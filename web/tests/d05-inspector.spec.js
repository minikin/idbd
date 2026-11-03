import { expect, test } from "@playwright/test";
import { fmtAlpha, fmtSigned } from "../src/format.js";
import { openApp, runUntil } from "./helpers.js";

// D05: selecting a feature reveals what its adaptive step size means.
test("D05 the inspector describes the selected input from Rust state", async ({ page }) => {
  await openApp(page);
  await runUntil(page, 300);

  await page.click("#weight-row-3");
  await expect(page.locator("#inspector-name")).toHaveText("x3");
  const state = await page.evaluate(() => window.idbdExplorer.experiment());
  const pairs = (await page.locator("#inspector-pairs").innerText()).toLowerCase();

  expect(pairs).toContain("true coefficient (known simulator information)");
  expect(pairs).toContain("idbd alpha (size of the learning adjustment)");
  expect(pairs).toContain(fmtAlpha(state.alphas[3]));
  expect(pairs).toContain(fmtSigned(state.coefficients[3], 3));

  // Alpha is never presented as importance or confidence.
  expect(pairs).not.toMatch(/importance|confidence/);
  const meaning = (await page.locator("#inspector-meaning").innerText()).toLowerCase();
  expect(meaning).toContain("size of the learning adjustment");
  expect(meaning).not.toMatch(/(?<!not its )importance/);

  const history = page.locator("#inspector-alpha [aria-label]").first();
  await expect(history).toHaveAttribute("aria-label", /x3, base-10 logarithmic scale/);
});

test("D05 keyboard selection moves the inspector", async ({ page }) => {
  await openApp(page);
  await runUntil(page, 50);
  await page.focus("#weight-row-0");
  await page.keyboard.press("ArrowDown");
  await expect(page.locator("#inspector-name")).toHaveText("x1");
});
