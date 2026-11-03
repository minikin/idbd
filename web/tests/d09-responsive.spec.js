import { expect, test } from "@playwright/test";

import { openApp, runUntil } from "./helpers.js";

const WIDTHS = [
  { width: 390, height: 844, name: "narrow" },
  { width: 1440, height: 900, name: "wide" },
];

async function horizontalOverflow(page) {
  return page.evaluate(() => {
    const root = document.documentElement;
    return {
      scrollWidth: root.scrollWidth,
      clientWidth: root.clientWidth,
      // A wide element inside its own scroll container, such as the values table,
      // is clipped there and never widens the page.
      widest: Array.from(document.querySelectorAll("body *"))
        .filter((node) => node.getBoundingClientRect().right > root.clientWidth + 1)
        .filter((node) => {
          for (let parent = node.parentElement; parent; parent = parent.parentElement) {
            if (getComputedStyle(parent).overflowX !== "visible") {
              return false;
            }
          }
          return true;
        })
        .slice(0, 5)
        .map((node) => `${node.tagName}.${node.className}`),
    };
  });
}

for (const size of WIDTHS) {
  test(`D09 no page-level horizontal scrolling at ${size.width} px`, async ({ page }) => {
    await page.setViewportSize({ width: size.width, height: size.height });
    await openApp(page);
    await runUntil(page, 150);
    await page.locator("#weight-row-2").click();

    const overflow = await horizontalOverflow(page);
    expect(overflow.widest).toEqual([]);
    expect(overflow.scrollWidth).toBeLessThanOrEqual(overflow.clientWidth + 1);
  });

  test(`D09 the same functions are available at ${size.width} px`, async ({ page }) => {
    await page.setViewportSize({ width: size.width, height: size.height });
    await openApp(page);

    for (const id of ["run", "pause", "step", "change-world", "reset", "apply-settings"]) {
      await expect(page.locator(`#${id}`)).toBeVisible();
    }
    for (const id of ["speed", "preset", "seed", "alpha0", "theta", "lms-step"]) {
      await expect(page.locator(`#${id}`)).toBeVisible();
    }
    for (const id of [
      "weights-panel",
      "error-panel",
      "alpha-panel",
      "events-panel",
      "inspector-panel",
      "table-panel",
    ]) {
      await expect(page.locator(`#${id}`)).toBeVisible();
    }
    // Exact values stay readable as text, not only as marks. On a phone the panels show
    // them from the first sample on.
    await page.click("#step");
    await page.locator("#weight-row-4").click();
    await expect(page.locator("#inspector-pairs")).toContainText("idbd alpha");
    await expect(page.locator("#state-table tbody tr")).toHaveCount(20);
  });
}

test("D09 on a phone, idle panels stay short and the run definition starts folded", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await openApp(page);

  await expect(page.locator("#run-details")).not.toHaveAttribute("open", "");
  await expect(page.locator("#weights-panel .idle-note")).toBeVisible();
  await expect(page.locator("#weight-list")).toBeHidden();
  const idle = await page.locator("#experiment").evaluate((node) => node.getBoundingClientRect().height);
  expect(idle).toBeLessThan(3000);

  await page.click("#step");
  await expect(page.locator("#weights-panel .idle-note")).toBeHidden();
  await expect(page.locator("#weight-list")).toBeVisible();
  await expect(page.locator("#state-table")).toBeVisible();

  await page.locator("#run-details > summary").click();
  await expect(page.locator("#run-header")).toContainText("theta 0.01,");
});
