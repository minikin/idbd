import AxeBuilder from "@axe-core/playwright";
import { expect, test } from "@playwright/test";

// S01 to S06: the story page. Its learning values come from the crate's HouseRace, the
// game races that crate code, and the page stays usable without WebGL or motion.

const TAGS = ["wcag2a", "wcag2aa", "wcag21a", "wcag21aa", "wcag22aa"];

async function openStory(page) {
  await page.goto("/story.html");
  await page.waitForSelector('body[data-app-ready="true"]', { timeout: 20000 });
}

const story = (page) => page.evaluate(() => ({
  house: window.idbdStory.house,
  renderer: window.idbdStory.renderer,
  hero: window.idbdStory.hero(),
  desk: window.idbdStory.desk,
  lanes: { idbd: window.idbdStory.lanes.idbd, lms: window.idbdStory.lanes.lms },
  game: window.idbdStory.game(),
}));

test("S01 the chapters show step sizes and errors computed by the crate", async ({ page }) => {
  const errors = [];
  page.on("pageerror", (error) => errors.push(String(error)));
  await openStory(page);
  const s = await story(page);
  expect(s.house.sensors).toHaveLength(12);
  expect(s.house.relevant).toBe(3);
  expect(s.hero.completed).toBeGreaterThanOrEqual(5000);

  const slowestRelevant = Math.min(...s.desk.slice(0, 3));
  const fastestInert = Math.max(...s.desk.slice(3));
  expect(slowestRelevant).toBeGreaterThan(3 * fastestInert);
  const labels = await page.$$eval("#desk .fader-label .mono", (nodes) => nodes.map((n) => n.textContent));
  expect(labels).toEqual(s.desk.map((a) => a.toFixed(3)));

  expect(Number.isFinite(s.lanes.idbd) && Number.isFinite(s.lanes.lms)).toBe(true);
  await expect(page.locator("#run-idbd")).toHaveText(s.lanes.idbd.toFixed(2));
  await expect(page.locator("#run-lms")).toHaveText(s.lanes.lms.toFixed(2));
  expect(await page.locator("#lanes svg").count()).toBe(4);
  expect(errors).toEqual([]);
});

test("S02 the game races the crate learners and resets completely", async ({ page }) => {
  await openStory(page);
  await expect(page.locator("#scores .note").first()).toHaveText("press Start");

  const sun = page.locator("#trust-0");
  await sun.focus();
  for (let i = 0; i < 5; i += 1) await page.keyboard.press("ArrowRight");
  await expect(page.locator('output[for="trust-0"]')).toHaveText("0.5");

  await page.click("#game-run");
  await expect(page.locator("#game-run")).toHaveText("Pause");
  await page.waitForFunction(() => window.idbdStory.game().scored >= 60, null, { timeout: 15000 });
  await page.click("#game-run");
  const paused = (await story(page)).game;
  expect(paused.running).toBe(false);
  expect(paused.trust[0]).toBe(0.5);
  for (const value of Object.values(paused.scores)) expect(Number.isFinite(value)).toBe(true);
  await page.waitForTimeout(400);
  expect((await story(page)).game.scored).toBe(paused.scored);
  await expect(page.locator("#scores .score[class*='lead-']")).toHaveCount(1);

  await page.click("#game-reset");
  const reset = (await story(page)).game;
  expect(reset.scored).toBe(0);
  expect(reset.trust.every((v) => v === 0)).toBe(true);
  expect(reset.revealed).toBe(false);
  await expect(page.locator('output[for="trust-0"]')).toHaveText("0.0");
});

test("S03 IDBD's speeds stay hidden until the visitor asks", async ({ page }) => {
  await openStory(page);
  await expect(page.locator("#learned")).toBeHidden();
  await expect(page.locator("#reveal")).toHaveAttribute("aria-expanded", "false");
  await page.click("#reveal");
  await expect(page.locator("#learned")).toBeVisible();
  await expect(page.locator("#reveal")).toHaveAttribute("aria-expanded", "true");
  const speeds = await page.$$eval("#learned .item .faint", (nodes) => nodes.map((n) => n.textContent));
  const game = (await story(page)).game;
  expect(speeds).toEqual(game.stepSizes.map((a) => `speed ${a.toFixed(3)}`));
});

test("S04 with reduced motion the house is the 2D drawing", async ({ page }) => {
  await page.emulateMedia({ reducedMotion: "reduce" });
  await openStory(page);
  expect((await story(page)).renderer).toBe("2d");
  await expect(page.locator("#house-3d")).toBeHidden();
  await expect(page.locator("#house-2d")).toBeVisible();
  expect(await page.locator("#house-2d-sensors circle").count()).toBe(24);
});

test("S05 the story page has no WCAG A or AA violations", async ({ page }) => {
  await openStory(page);
  await page.click("#game-run");
  await page.waitForFunction(() => window.idbdStory.game().scored >= 20, null, { timeout: 15000 });
  await page.click("#game-run");
  await page.click("#reveal");
  const results = await new AxeBuilder({ page }).withTags(TAGS).analyze();
  const found = results.violations.map((v) => ({ id: v.id, targets: v.nodes.map((n) => n.target.join(" ")) }));
  expect(found).toEqual([]);
});

test("S06 at phone width nothing scrolls sideways", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await openStory(page);
  const overflow = await page.evaluate(() => document.documentElement.scrollWidth - window.innerWidth);
  expect(overflow).toBeLessThanOrEqual(0);
});
