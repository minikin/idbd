#!/usr/bin/env node
// Captures the demo at a narrow and a wide viewport for design review.
// Usage: node screenshots.mjs [out-dir] [url]

import { mkdir } from "node:fs/promises";
import { chromium } from "@playwright/test";

const OUT = process.argv[2] ?? "../internal/screenshots";
const URL = process.argv[3] ?? "http://127.0.0.1:8173/explorer.html";
const SIZES = [
  { name: "390", width: 390, height: 844 },
  { name: "1440", width: 1440, height: 900 },
];

await mkdir(OUT, { recursive: true });
const browser = await chromium.launch();

for (const size of SIZES) {
  const page = await browser.newPage({
    locale: "en-US",
    viewport: { width: size.width, height: size.height },
    deviceScaleFactor: 2,
  });
  await page.goto(URL);
  await page.waitForSelector('body[data-app-ready="true"]');
  await page.waitForTimeout(800);

  await page.screenshot({ path: `${OUT}/initial-${size.name}.png` });

  await page.click("#guided-advance");
  await page.click("#guided-advance");
  await page.locator("#guided").screenshot({ path: `${OUT}/guided-${size.name}.png` });

  await page.selectOption("#speed", "5000");
  await page.click("#run");
  await page.waitForFunction(() => window.idbdExplorer.experiment().sample >= 4000, null, {
    timeout: 60000,
  });
  await page.click("#pause");
  await page.locator("#weight-row-2").click();
  await page.waitForTimeout(300);
  await page.locator("#experiment").screenshot({ path: `${OUT}/experiment-${size.name}.png` });

  await page.close();
  process.stdout.write(`captured ${size.name}\n`);
}

await browser.close();
