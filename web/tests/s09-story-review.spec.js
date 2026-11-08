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
  await expect(page.locator("#live-text")).toContainText("Day 8, 00:59: the meter read");
});

test("S09 every attention ring is drawn at 3:1 or better", async ({ page }) => {
  await page.emulateMedia({ reducedMotion: "reduce" });
  await openStory(page);
  const opacities = await page.$$eval(".house-2d .attention", (rings) => rings.map((r) => Number(r.getAttribute("stroke-opacity"))));
  expect(opacities).toHaveLength(12);
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

test("S09 chapter 2 traces a late and a nervous fixed speed against IDBD", async ({ page }) => {
  await openStory(page);
  const speeds = await page.evaluate(() => window.idbdStory.speeds);
  expect(speeds.window.length).toBe(180);
  // The tuned fixed speed never gets halfway to the small-reading sensor's effect, IDBD does.
  expect(speeds.late.lmsHalf).toBeNull();
  expect(speeds.late.peak).toBeLessThan(0.6);
  expect(speeds.late.idbdHalf).toBeLessThan(60);
  // On a sensor that never matters the faster fixed speed swings far more than IDBD.
  expect(speeds.nervous.lmsSpread).toBeGreaterThan(3 * speeds.nervous.idbdSpread);
  await expect(page.locator("#speed-low-value")).toHaveText("0.01");
  await expect(page.locator("#speed-high-value")).toHaveText("0.05");
  await expect(page.locator("#speed-low")).toHaveAttribute("aria-label", /^Measured over 120 minutes/);
  await expect(page.locator("#late-caption")).toContainText(`IDBD gets halfway in ${speeds.late.idbdHalf} minutes`);
  await expect(page.locator("#nervous-caption")).toContainText(`with a spread of ${speeds.nervous.lmsSpread.toFixed(2)}`);
  await expect(page.locator("#speed-high polyline")).toHaveCount(3);
});

test("S09 slow motion reports exact speed changes and resets cleanly", async ({ page }) => {
  await openStory(page);
  await expect(page.locator("#slow-summary")).toContainText("the same step size, 0.0500.");
  await expect(page.locator("#slow-summary")).toContainText("16 times faster on the sensors whose readings swing most");
  await page.click("#slow-next");
  const one = await slow(page);
  expect(one.last.after).toEqual(one.last.before);
  await expect(page.locator("#slow-guess")).toHaveText("at the base");
  await expect(page.locator("#slow-summary")).toContainText("No speed moved");
  await expect(page.locator("#slow-rows td:nth-child(4)").first()).toContainText("unchanged");

  await page.click("#slow-next");
  const two = await slow(page);
  const up = two.last.after.filter((a, s) => a > two.last.before[s]).length;
  const down = two.last.after.filter((a, s) => a < two.last.before[s]).length;
  expect(up + down).toBe(12);
  await expect(page.locator("#slow-summary")).toContainText(`${up} speeds went up and ${down} went down`);

  await page.click("#slow-restart");
  const classes = await page.$$eval("#slow-rows td:nth-child(4)", (cells) => cells.map((c) => c.className));
  expect(classes).toEqual(Array(12).fill(""));
});

test("S09 the speed sparklines share one scale", async ({ page }) => {
  await openStory(page);
  await page.click("#slow-run");
  const ends = await page.$$eval("#slow-rows polyline", (lines) =>
    lines.map((line) => Number(line.getAttribute("points").split(" ").at(-1).split(",")[1])),
  );
  // A smaller y is higher on screen: every sensor that matters ends above every other one.
  const { order } = await slow(page);
  const matters = await page.evaluate(() => window.idbdStory.home.sensors.map((s) => s.hours !== 0));
  const ofRow = (_, row) => matters[order[row]];
  expect(Math.max(...ends.filter(ofRow))).toBeLessThan(Math.min(...ends.filter((e, row) => !ofRow(e, row))));
});

test("S09 slow motion letters its sensors until the game is played", async ({ page }) => {
  await page.emulateMedia({ reducedMotion: "reduce" });
  await openStory(page);
  const names = await page.evaluate(() => window.idbdStory.home.sensors.map((s) => s.name));
  const heads = () => page.$$eval("#slow-rows th", (cells) => cells.map((c) => c.textContent));
  expect(await heads()).toEqual([..."ABCDEFGHIJKL"].map((l) => `Sensor ${l}`));
  await page.click("#slow-run");
  for (const name of names) await expect(page.locator("#slow-summary")).not.toContainText(name);
  await expect(page.locator("#slow-hidden")).toBeVisible();

  await page.click("#family-run");
  await expect(page.locator("#family-result")).toBeVisible();
  const { order, named } = await slow(page);
  expect(named).toBe(true);
  expect(await heads()).toEqual(order.map((s) => names[s]));
  await expect(page.locator("#slow-hidden")).toBeHidden();
});

test("S09 the slow-motion explanation is announced", async ({ page }) => {
  await openStory(page);
  const live = await page.locator("#slow-summary").evaluate((n) => n.closest("[aria-live]")?.getAttribute("aria-live"));
  expect(live).toBe("polite");
});

test("S09 on a phone the house key names every sensor", async ({ page }) => {
  await page.emulateMedia({ reducedMotion: "reduce" });
  await page.setViewportSize({ width: 390, height: 844 });
  await openStory(page);
  await expect(page.locator("#house-key li")).toHaveCount(12);
  await expect(page.locator("#house-key")).toBeVisible();

  await page.setViewportSize({ width: 1440, height: 900 });
  await expect(page.locator("#house-key")).toBeHidden();
  await expect
    .poll(() => page.$$eval(".house-2d .label-chip", (chips) => Math.min(...chips.map((c) => Number(c.getAttribute("width"))))))
    .toBeGreaterThan(30);
});

test("S09 the sensor grid carries its values as text", async ({ page }) => {
  await openStory(page);
  await expect(page.locator("#sensor-grid li .sr-only").first()).toContainText("latest reading");
});
