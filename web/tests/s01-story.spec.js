import AxeBuilder from "@axe-core/playwright";
import { expect, test } from "@playwright/test";

// S01 to S07: the story page. Its learning values come from the crate's HouseRace, the
// game races that crate code, and the page stays usable without WebGL or motion.

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
    lanes: { idbd: window.idbdStory.lanes.idbd, lms: window.idbdStory.lanes.lms },
    game: window.idbdStory.game(),
    slow: window.idbdStory.slow(),
  }));

async function setSlider(page, sensor, value) {
  await page.locator(`#trust-${sensor}`).evaluate((input, v) => {
    input.value = String(v);
    input.dispatchEvent(new Event("input", { bubbles: true }));
  }, value);
}

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

  await expect(page.locator("#run-idbd")).toHaveText(`${Math.sqrt(s.lanes.idbd).toFixed(2)} \u00b0C`);
  await expect(page.locator("#run-lms")).toHaveText(`${Math.sqrt(s.lanes.lms).toFixed(2)} \u00b0C`);
  expect(errors).toEqual([]);
});

test("S02 slow motion shows one crate update at a time", async ({ page }) => {
  await openStory(page);
  await expect(page.locator("#slow-reading")).toHaveText("0");
  await page.click("#slow-next");
  await expect(page.locator("#slow-reading")).toHaveText("1");
  const one = (await story(page)).slow;
  const sign = (v) => `${v >= 0 ? "+" : "−"}${Math.abs(v).toFixed(2)}`;
  await expect(page.locator("#slow-guess")).toHaveText(`${sign(one.last.guess)} \u00b0C`);
  await expect(page.locator("#slow-actual")).toHaveText(`${sign(one.last.target)} \u00b0C`);
  await expect(page.locator("#slow-miss")).toHaveText(`${sign(one.last.target - one.last.guess)} \u00b0C`);
  await expect(page.locator("#slow-summary")).toContainText("IDBD guessed");

  await page.click("#slow-run");
  await expect(page.locator("#slow-reading")).toHaveText("5,001");
  const later = (await story(page)).slow;
  const speeds = await page.$$eval("#slow-rows td:nth-child(4)", (cells) => cells.map((c) => c.textContent));
  speeds.forEach((text, s) => expect(text).toContain(`→ ${later.stepSizes[s].toFixed(4)}`));

  await page.click("#slow-restart");
  await expect(page.locator("#slow-reading")).toHaveText("0");
});

test("S03 the calm house rewards matching the truth, and Reset clears everything", async ({ page }) => {
  await openStory(page);
  await expect(page.locator(".level[data-level='changing']")).toHaveAttribute("aria-pressed", "true");
  await page.click(".level[data-level='still']");
  await expect(page.locator(".level[data-level='still']")).toHaveAttribute("aria-pressed", "true");
  const fresh = (await story(page)).game;
  expect(fresh.level).toBe("still");
  expect(fresh.lmsStepSize).toBe((await story(page)).house.stillLmsStepSize);

  await expect(page.locator("#trust-0-cost")).toHaveText("you say: ignore it");
  await page.click("#hints");
  for (const [sensor, value] of [[0, 1.2], [1, 0.9], [2, -1.1]]) await setSlider(page, sensor, value);
  const tuned = (await story(page)).game;
  expect(tuned.costs.every((c) => c < 0.005)).toBe(true);
  await expect(page.locator("#trust-0-cost")).toHaveText("you say: warms the room a lot \u2713 close");

  await setSlider(page, 3, 1.5);
  await expect(page.locator("#trust-3-cost")).toHaveText("you say: warms the room a lot \u00b7 \u2193 try lower");

  await page.click("#game-run");
  await page.waitForFunction(() => window.idbdStory.game().scored >= 60, null, { timeout: 20000 });
  await page.click("#game-run");
  const paused = (await story(page)).game;
  expect(paused.running).toBe(false);
  for (const value of Object.values(paused.scores)) expect(Number.isFinite(value)).toBe(true);
  await page.waitForTimeout(400);
  expect((await story(page)).game.scored).toBe(paused.scored);

  await page.click("#game-reset");
  const reset = (await story(page)).game;
  expect(reset.scored).toBe(0);
  expect(reset.trust.every((v) => v === 0)).toBe(true);
  expect(reset.revealed).toBe(false);
});

test("S04 level 1 races on the switching house with its own fixed speed", async ({ page }) => {
  await openStory(page);
  await page.click(".level[data-level='still']");
  await page.click(".level[data-level='changing']");
  await expect(page.locator(".level[data-level='changing']")).toHaveAttribute("aria-pressed", "true");
  const s = await story(page);
  expect(s.game.level).toBe("changing");
  expect(s.game.lmsStepSize).toBe(s.house.lmsStepSize);
  await expect(page.locator("#scores .note").nth(2)).toContainText("one speed for every sensor");
});

test("S05 IDBD's trust and speeds stay hidden until the visitor asks", async ({ page }) => {
  await openStory(page);
  await expect(page.locator("#learned")).toBeHidden();
  await page.click("#reveal");
  await expect(page.locator("#learned")).toBeVisible();
  await expect(page.locator("#reveal")).toHaveAttribute("aria-expanded", "true");
  const speeds = await page.$$eval("#learned .item .faint", (nodes) => nodes.map((n) => n.textContent));
  const game = (await story(page)).game;
  speeds.forEach((text, s) => expect(text).toContain(`(speed ${game.stepSizes[s].toFixed(3)},`));
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
  await page.click("#game-run");
  await page.waitForFunction(() => window.idbdStory.game().scored >= 20, null, { timeout: 15000 });
  await page.click("#game-run");
  await page.click("#reveal");
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

test("S08 a round ends after 600 readings with a winner and a reason", async ({ page }) => {
  test.setTimeout(120_000);
  await page.clock.install();
  await openStory(page);
  await page.click("#game-run");
  for (let second = 0; second < 61; second += 1) await page.clock.runFor(1_000);
  const game = (await story(page)).game;
  expect(game.finished).toBe(true);
  expect(game.scored).toBe(600);
  await expect(page.locator("#verdict")).toBeVisible();
  await expect(page.locator("#verdict")).toContainText("Accuracy: ");
  const accuracy = (await story(page)).game.accuracy;
  for (const value of Object.values(accuracy)) expect(value).toBeGreaterThanOrEqual(0);
  await expect(page.locator("#scores .value").nth(1)).toHaveText(`${Math.round(accuracy.idbd * 100)}%`);
  await expect(page.locator("#game-run")).toHaveText("Play again");
  await expect(page.locator("#scores .lead", { hasText: "winner" })).toHaveCount(1);
});
