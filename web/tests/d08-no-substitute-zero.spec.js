import { expect, test } from "@playwright/test";
import { openApp } from "./helpers.js";

// D08: no missing or non-finite result is replaced with zero. Before the first sample there
// is no pre-update error or rolling MSE, and the page must say so.
test("D08 a fresh run shows missing measurements as missing, not zero", async ({ page }) => {
  await openApp(page);
  const state = await page.evaluate(() => window.idbdExplorer.experiment());
  expect(state.sample).toBe(0);
  expect(state.idbdError).toBeNull();
  expect(state.lmsError).toBeNull();
  expect(state.idbdRollingMse).toBeNull();
  expect(state.lmsRollingMse).toBeNull();

  const text = await page.locator("#experiment").innerText();
  for (const label of [
    "idbd pre-update error",
    "lms pre-update error",
    "idbd rolling mse",
    "lms rolling mse",
  ]) {
    expect(text).toMatch(new RegExp(`${label}[^\\n]*\\n?\\s*no sample yet`, "i"));
  }

  await page.click("#step");
  const after = await page.evaluate(() => window.idbdExplorer.experiment());
  expect(after.sample).toBe(1);
  expect(Number.isFinite(after.lmsError)).toBe(true);
  expect(Number.isFinite(after.lmsRollingMse)).toBe(true);
});
