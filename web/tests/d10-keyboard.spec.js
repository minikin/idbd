import { expect, test } from "@playwright/test";

import { completedSample, openApp } from "./helpers.js";

/** Presses Tab until the focused element matches `id`, or fails. */
async function tabTo(page, id) {
  for (let press = 0; press < 60; press += 1) {
    if ((await page.evaluate(() => document.activeElement.id)) === id) {
      return press;
    }
    await page.keyboard.press("Tab");
  }
  throw new Error(`could not reach #${id} with the keyboard`);
}

test("D10 the experiment runs, pauses and steps from the keyboard alone", async ({ page }) => {
  await openApp(page);

  await tabTo(page, "run");
  await page.keyboard.press("Enter");
  await page.waitForFunction(() => window.idbdExplorer.isRunning() === true);

  await tabTo(page, "pause");
  await page.keyboard.press("Enter");
  await page.waitForFunction(() => window.idbdExplorer.isRunning() === false);
  const paused = await completedSample(page);

  await tabTo(page, "step");
  await page.keyboard.press("Enter");
  expect(await completedSample(page)).toBe(paused + 1);

  await tabTo(page, "change-world");
  await page.keyboard.press("Enter");
  expect((await page.locator("#playback-state").innerText()).toLowerCase()).toContain(
    "world change applies to the next sample",
  );
});

test("D10 the focused control has a visible focus ring and an accessible name", async ({
  page,
}) => {
  await openApp(page);
  await tabTo(page, "run");

  const focus = await page.evaluate(() => {
    const style = getComputedStyle(document.activeElement);
    return { width: Number.parseFloat(style.outlineWidth), style: style.outlineStyle };
  });
  expect(focus.width).toBeGreaterThan(0);
  expect(focus.style).not.toBe("none");

  const unnamed = await page.$$eval("#experiment button, #experiment select, #experiment input", (nodes) =>
    nodes
      .filter((node) => {
        const label = node.getAttribute("aria-label");
        const text = node.textContent.trim();
        const described = node.labels && node.labels.length > 0;
        return !label && !text && !described;
      })
      .map((node) => node.outerHTML.slice(0, 80)),
  );
  expect(unnamed).toEqual([]);
});

test("D10 a feature is selectable with the arrow keys", async ({ page }) => {
  await openApp(page);
  await page.click("#step");
  await tabTo(page, "weight-row-0");

  await page.keyboard.press("ArrowDown");
  await page.keyboard.press("ArrowDown");
  expect(await page.evaluate(() => document.activeElement.id)).toBe("weight-row-2");
  expect(await page.locator("#weight-row-2").getAttribute("aria-selected")).toBe("true");
  expect(await page.locator("#inspector-name").innerText()).toBe("x2");

  await page.keyboard.press("End");
  expect(await page.locator("#inspector-name").innerText()).toBe("x19");

  // The bars are never the only reading: the row carries the numbers in its name.
  const label = await page.locator("#weight-row-19").getAttribute("aria-label");
  expect(label).toContain("IDBD weight");
  expect(label).toContain("true coefficient");
  expect(label).toContain("alpha");
});

test("D10 labels and shapes distinguish IDBD, LMS and the true coefficients", async ({ page }) => {
  await openApp(page);

  const legend = await page.locator("#weights-legend").innerText();
  expect(legend).toContain("IDBD weight");
  expect(legend).toContain("LMS weight");
  expect(legend).toContain("true coefficient (simulator)");

  const alphaLegend = await page.locator("#alpha-legend").innerText();
  expect(alphaLegend).toContain("currently relevant input");
  expect(alphaLegend).toContain("irrelevant input");
});

test("D10 reduced motion is respected", async ({ page }) => {
  await page.emulateMedia({ reducedMotion: "reduce" });
  await openApp(page);

  const duration = await page.evaluate(() =>
    getComputedStyle(document.getElementById("run")).transitionDuration,
  );
  expect(Number.parseFloat(duration)).toBeLessThan(0.05);
});
