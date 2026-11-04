import { expect, test } from "@playwright/test";

// S09: fixes from the third review of the story page.

async function openStory(page) {
  await page.goto("/");
  await page.waitForSelector('body[data-app-ready="true"]', { timeout: 20000 });
}

const hero = (page) => page.evaluate(() => window.idbdStory.hero());
const slow = (page) => page.evaluate(() => window.idbdStory.slow());

test("S09 the hero stops when it is off screen and when paused", async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  await openStory(page);
  await expect.poll(async () => (await hero(page)).running).toBe(true);

  await page.locator("#play").scrollIntoViewIfNeeded();
  await expect.poll(async () => (await hero(page)).running).toBe(false);
  const parked = (await hero(page)).completed;
  await page.waitForTimeout(600);
  expect((await hero(page)).completed).toBe(parked);

  await page.evaluate(() => window.scrollTo(0, 0));
  await expect.poll(async () => (await hero(page)).running).toBe(true);
  await page.click("#hero-pause");
  await expect(page.locator("#hero-pause")).toHaveText("Play");
  const paused = (await hero(page)).completed;
  await page.waitForTimeout(600);
  expect((await hero(page)).completed).toBe(paused);
  await page.click("#hero-pause");
  await expect.poll(async () => (await hero(page)).completed).toBeGreaterThan(paused);
});

test("S09 reduced motion starts on a still frame with a Play button", async ({ page }) => {
  await page.emulateMedia({ reducedMotion: "reduce" });
  await openStory(page);
  const first = await hero(page);
  expect(first.paused).toBe(true);
  await expect(page.locator("#hero-pause")).toHaveText("Play");
  await page.waitForTimeout(600);
  expect((await hero(page)).completed).toBe(first.completed);
  const transition = await page.locator(".house-2d .attention").first().evaluate((n) => getComputedStyle(n).transitionDuration);
  expect(transition).toBe("0s");
  await expect(page.locator("#live-text")).toContainText("Minute 5,060");
});

test("S09 every attention ring is drawn at 3:1 or better", async ({ page }) => {
  await page.emulateMedia({ reducedMotion: "reduce" });
  await openStory(page);
  const opacities = await page.$$eval(".house-2d .attention", (rings) => rings.map((r) => Number(r.getAttribute("stroke-opacity"))));
  expect(opacities).toHaveLength(6);
  for (const opacity of opacities) expect(opacity).toBeGreaterThanOrEqual(0.5);
});

test("S09 a lost WebGL context hands the house to the 2D drawing", async ({ page }) => {
  await openStory(page);
  test.skip((await page.evaluate(() => window.idbdStory.renderer)) !== "3d", "no WebGL in this browser");
  await page.evaluate(() => {
    const canvas = document.querySelector("#house-3d canvas");
    const gl = canvas.getContext("webgl2") ?? canvas.getContext("webgl");
    gl.getExtension("WEBGL_lose_context").loseContext();
  });
  await expect.poll(() => page.evaluate(() => window.idbdStory.renderer)).toBe("2d");
  await expect(page.locator("#house-3d")).toBeHidden();
  await expect(page.locator("#house-2d")).toBeVisible();
  await expect(page.locator("#house-2d")).not.toHaveAttribute("aria-hidden", "true");
});

test("S09 chapter 2 plots a measured oven switch at two fixed speeds", async ({ page }) => {
  await openStory(page);
  const speeds = await page.evaluate(() => window.idbdStory.speeds);
  expect(speeds.truth[39]).toBe(0.1);
  expect(speeds.truth[40]).toBe(0.9);
  expect(speeds.truth.at(-1)).toBe(0.9);
  // The low speed is still well short of the new effect at the end, the high speed swings.
  expect(speeds.low.at(-1)).toBeLessThan(0.8);
  const tail = speeds.high.slice(100);
  expect(Math.max(...tail) - Math.min(...tail)).toBeGreaterThan(0.6);
  await expect(page.locator("#speed-low-value")).toHaveText("0.005");
  await expect(page.locator("#speed-high-value")).toHaveText("0.2");
  await expect(page.locator("#speed-low")).toHaveAttribute("aria-label", /^Measured: the oven's real effect jumps from 0.1 to 0.9/);
  await expect(page.locator("#speed-high polyline")).toHaveCount(2);
});

test("S09 slow motion reports exact speed changes and resets cleanly", async ({ page }) => {
  await openStory(page);
  await expect(page.locator("#slow-summary")).toContainText("the same speed, 0.0500.");
  await page.click("#slow-next");
  const one = await slow(page);
  expect(one.last.after).toEqual(one.last.before);
  await expect(page.locator("#slow-guess")).toHaveText("no change");
  await expect(page.locator("#slow-summary")).toContainText("No speed moved");
  await expect(page.locator("#slow-rows td:nth-child(4)").first()).toContainText("unchanged");

  await page.click("#slow-next");
  const two = await slow(page);
  const up = two.last.after.filter((a, s) => a > two.last.before[s]).length;
  const down = two.last.after.filter((a, s) => a < two.last.before[s]).length;
  expect(up + down).toBe(6);
  await expect(page.locator("#slow-summary")).toContainText(`${up} speeds went up and ${down} went down`);

  await page.click("#slow-restart");
  const classes = await page.$$eval("#slow-rows td:nth-child(4)", (cells) => cells.map((c) => c.className));
  expect(classes).toEqual(["", "", "", "", "", ""]);
});

test("S09 the speed sparklines share one scale", async ({ page }) => {
  await openStory(page);
  await page.click("#slow-run");
  const ends = await page.$$eval("#slow-rows polyline", (lines) =>
    lines.map((line) => Number(line.getAttribute("points").split(" ").at(-1).split(",")[1])),
  );
  // A smaller y is higher on screen: every relevant sensor ends above every inert one.
  expect(Math.max(...ends.slice(0, 3))).toBeLessThan(Math.min(...ends.slice(3)));
});

test("S09 the slow-motion explanation is announced", async ({ page }) => {
  await openStory(page);
  const live = await page.locator("#slow-summary").evaluate((n) => n.closest("[aria-live]")?.getAttribute("aria-live"));
  expect(live).toBe("polite");
});

test("S09 on a phone the 2D house names its sensors and the lanes footer fits", async ({ page }) => {
  await page.emulateMedia({ reducedMotion: "reduce" });
  await page.setViewportSize({ width: 390, height: 844 });
  await openStory(page);
  await expect(page.locator("#house-key li")).toHaveCount(6);
  await expect(page.locator("#house-key")).toBeVisible();
  const axis = page.locator("#lanes .mono.faint").first();
  const box = await axis.boundingBox();
  expect(box.height).toBeLessThan(24);

  await page.setViewportSize({ width: 1440, height: 900 });
  await expect(page.locator("#house-key")).toBeHidden();
  const widths = await page.$$eval(".house-2d .label-chip", (chips) => chips.map((c) => Number(c.getAttribute("width"))));
  for (const width of widths) expect(width).toBeGreaterThan(40);
});

test("S09 the sensor grid and lanes carry their values as text", async ({ page }) => {
  await openStory(page);
  await expect(page.locator("#sensor-grid li .sr-only").first()).toContainText("latest reading");
  await expect(page.locator("#lanes .lane-name .sr-only").first()).toContainText(/strong for \d+ of 600 readings/);
  await expect(page.locator("#lanes .lane-name .sr-only").last()).toContainText("never affects the room");
});
