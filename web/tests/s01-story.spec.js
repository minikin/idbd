import AxeBuilder from "@axe-core/playwright";
import { expect, test } from "@playwright/test";

import { inWords, missInWords } from "../story/slow.js";

// S01 to S07: the story page. Its learning values come from the crate's HouseRace, and
// the page stays usable without WebGL or motion. The game has its own file, s10.

const TAGS = ["wcag2a", "wcag2aa", "wcag21a", "wcag21aa", "wcag22aa"];

async function openStory(page) {
  await page.goto("/");
  await page.waitForSelector('body[data-app-ready="true"]', { timeout: 20000 });
}

const story = (page) =>
  page.evaluate(() => ({
    house: window.idbdStory.house,
    renderer: window.idbdStory.renderer,
    hero: window.idbdStory.hero(),
    desk: window.idbdStory.desk,
    lanes: { idbd: window.idbdStory.lanes.idbdAccuracy, lms: window.idbdStory.lanes.lmsAccuracy },
    game: window.idbdStory.game(),
    slow: window.idbdStory.slow(),
  }));

test("S01 the chapters show step sizes and errors computed by the crate", async ({ page }) => {
  const errors = [];
  page.on("pageerror", (error) => errors.push(String(error)));
  await openStory(page);
  const s = await story(page);
  expect(s.house.sensors).toHaveLength(6);
  expect(s.house.relevant).toBe(3);
  expect(s.hero.completed).toBeGreaterThanOrEqual(5000);
  await expect(page.locator("#sensor-grid li")).toHaveCount(6);

  const slowestRelevant = Math.min(...s.desk.slice(0, 3));
  const fastestInert = Math.max(...s.desk.slice(3));
  expect(slowestRelevant).toBeGreaterThan(3 * fastestInert);
  const labels = await page.$$eval("#desk .fader-label .mono", (nodes) => nodes.map((n) => n.textContent));
  expect(labels).toEqual(s.desk.map((a) => a.toFixed(3)));

  await expect(page.locator("#run-idbd")).toHaveText(`${Math.round(s.lanes.idbd * 100)}%`);
  await expect(page.locator("#run-lms")).toHaveText(`${Math.round(s.lanes.lms * 100)}%`);
  expect(errors).toEqual([]);
});

test("S02 slow motion shows one crate update at a time", async ({ page }) => {
  await openStory(page);
  await expect(page.locator("#slow-reading")).toHaveText("0");
  await page.click("#slow-next");
  await expect(page.locator("#slow-reading")).toHaveText("1");
  const one = (await story(page)).slow;
  await expect(page.locator("#slow-guess")).toHaveText(inWords(one.last.guess));
  await expect(page.locator("#slow-actual")).toHaveText(inWords(one.last.target));
  await expect(page.locator("#slow-miss")).toHaveText(missInWords(one.last.target, one.last.guess));
  await expect(page.locator("#slow-summary")).toContainText("IDBD guessed");

  await page.click("#slow-run");
  await expect(page.locator("#slow-reading")).toHaveText("5,001");
  const later = (await story(page)).slow;
  const speeds = await page.$$eval("#slow-rows td:nth-child(4)", (cells) => cells.map((c) => c.textContent));
  speeds.forEach((text, s) => expect(text).toContain(`→ ${later.stepSizes[s].toFixed(4)}`));

  await page.click("#slow-restart");
  await expect(page.locator("#slow-reading")).toHaveText("0");
});

test("S06 with reduced motion the house is the 2D drawing", async ({ page }) => {
  await page.emulateMedia({ reducedMotion: "reduce" });
  await openStory(page);
  expect((await story(page)).renderer).toBe("2d");
  await expect(page.locator("#house-3d")).toBeHidden();
  await expect(page.locator("#house-2d")).toBeVisible();
  await expect(page.locator("#house-2d-sensors .attention")).toHaveCount(6);
  await expect(page.locator("#house-2d-sensors .label-chip")).toHaveCount(6);
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
