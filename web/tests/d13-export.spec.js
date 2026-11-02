import { readFile } from "node:fs/promises";

import { expect, test } from "@playwright/test";

import { openApp, runUntil } from "./helpers.js";

test("D13 the export describes the run and its results", async ({ page }) => {
  await openApp(page);
  await runUntil(page, 150);
  await page.click("#change-world");
  await page.click("#step");

  const [download] = await Promise.all([
    page.waitForEvent("download"),
    page.click("#export"),
  ]);
  const path = await download.path();
  const run = JSON.parse(await readFile(path, "utf8"));

  expect(download.suggestedFilename()).toContain("idbd-run-sutton1992-seed1-sample");
  expect(run.data).toContain("synthetic");
  expect(run.build.rustCoreVersion).toMatch(/\d+\.\d+\.\d+/);
  expect(run.build.generator).toContain("xoshiro256");
  expect(run.build.reproduce.join(" ")).toContain("just web-build");

  expect(run.settings.preset).toBe("sutton1992");
  expect(run.settings.seed).toBe(1);
  expect(run.settings.idbd.initialStepSize).toBe(0.05);
  expect(run.settings.idbd.theta).toBe(0.01);
  expect(run.settings.lms.stepSize).toBe(0.03);
  expect(run.settings.lms.selection).toContain("Experiment 1 grid");

  expect(run.samples.completed).toBeGreaterThan(150);
  expect(run.samples.rollingWindow).toBe(100);
  expect(run.samples.retainedWindow.last).toBe(run.samples.completed);
  expect(run.metrics.idbdWeights.length).toBe(20);
  expect(run.metrics.idbdRollingMse).toBeGreaterThan(0);
  expect(run.failure).toBe(null);

  const samples = run.events.map((event) => event.sample);
  expect(samples).toEqual([...samples].sort((a, b) => a - b));
  expect(
    run.events.some((event) => event.kind === "relevanceShift" && event.origin === "requested"),
  ).toBe(true);
});
