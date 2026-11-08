// Renders the README images from the real demo:
//   node docs/assets/hero/render.mjs          writes docs/assets/demo-loop.webp
//   node docs/assets/hero/render.mjs --hero   also writes hero-dark.webp and hero-light.webp
// The hero still is out of the README until it is redesigned, so it is opt-in.
//
// The still is one composite of three captures: the story page's house, chapter 2's
// measured fixed-speed traces and the Explorer's step-size chart after 20,000 samples.
// The loop is the guessing game: a visitor's guess, three days on the dial while IDBD's
// bars form, then the scores. It is captured under Playwright's fake clock so every frame
// lands on the same simulated time on every run. Run `just web-build` first. Needs
// cwebp and img2webp (libwebp).

import { execFileSync, spawn } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { createRequire } from "node:module";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, "../../..");
const assets = resolve(here, "..");
const { chromium } = createRequire(join(root, "web/package.json"))("@playwright/test");

const PORT = 8191;
const BASE = `http://127.0.0.1:${PORT}`;
const EXPLORER_SAMPLES = 20000;
const LOOP_FRAME_MS = 50;
// Simulated time between frames. Shorter than a frame, so the three days play slower than on
// the page and the hand moves about twenty minutes of the day per frame.
const LOOP_CLOCK_MS = 20;
// How long the loop holds on the visitor's guess before the hand starts, and on the scores
// before it starts again.
const LOOP_START_MS = 900;
const LOOP_HOLD_MS = 4000;
// A visitor's guess, [sensor, from, until], one right sensor with roughly right hours and
// two sensors marked during hours where nothing happens, so the scores differ.
const LOOP_GUESS = [
  [1, 5, 8],
  [5, 18, 22],
  [7, 18, 22],
];

const TRANSPARENT_PAGE = `
  html, body, main, .hero, .hero-visual { background: transparent !important; }
  body::before, body::after, main::before, main::after { display: none !important; }`;
// The loop shows the dial and two scores, the visitor's and IDBD's. The fixed speed's tile is
// left out because the loop cannot carry the page's note beside it: on rescaled readings a
// fixed speed scores 89% to IDBD's 85% over ten runs.
const LOOP_ONLY = `
  #family-scores .score:has(.swatch-lms) { display: none !important; }
  .site-header, .family-explain, .family-result .actions, #family-dial-caption { display: none !important; }
  body::before, body::after, main::before, main::after { display: none !important; }
  .family-stage { min-height: 470px; align-items: center !important; }`;

const work = mkdtempSync(join(tmpdir(), "idbd-hero-"));
const server = spawn(process.execPath, [join(root, "web/serve.mjs"), String(PORT)], { stdio: "ignore" });

async function waitForServer() {
  for (let i = 0; i < 50; i += 1) {
    try {
      if ((await fetch(BASE)).ok) return;
    } catch {
      // Not listening yet.
    }
    await new Promise((done) => setTimeout(done, 100));
  }
  throw new Error(`the demo server did not start on ${BASE}`);
}

async function openStory(browser, options = {}) {
  const page = await browser.newPage({ viewport: { width: 1440, height: 900 }, deviceScaleFactor: 2, ...options });
  return page;
}

async function captureStill(browser) {
  const page = await openStory(browser);
  await page.goto(`${BASE}/`);
  await page.waitForSelector('body[data-app-ready="true"]', { timeout: 30000 });
  await page.click("#hero-pause");
  // A transparent capture, so the composite's own card colour shows around the house
  // instead of a box of the page's background and glow.
  await page.addStyleTag({ content: TRANSPARENT_PAGE });
  await page.waitForTimeout(800);
  const house = await page.locator("#house").screenshot({ omitBackground: true });
  const speeds = await page.locator(".two-up").screenshot();

  await page.goto(`${BASE}/explorer.html`);
  await page.waitForSelector('body[data-app-ready="true"]', { timeout: 30000 });
  await page.selectOption("#speed", "1000");
  await page.click("#run");
  await page.waitForFunction((n) => window.idbdExplorer.experiment().sample >= n, EXPLORER_SAMPLES, {
    timeout: 60000,
  });
  await page.click("#pause");
  await page.waitForFunction(() => window.idbdExplorer.isRunning() === false);
  const alpha = await page.locator("#alpha-chart").screenshot();
  await page.close();
  return { house, speeds, alpha };
}

const png = (buffer) => `data:image/png;base64,${buffer.toString("base64")}`;

function composite({ house, speeds, alpha }, scheme) {
  const light = scheme === "light";
  return `<!doctype html><html><head><meta charset="utf-8">
<link rel="stylesheet" href="https://fonts.googleapis.com/css2?family=Geist:wght@400;600&family=Geist+Mono&display=block">
<style>
  * { box-sizing: border-box; }
  body { margin: 0; width: 1600px; height: 900px; padding: 44px 48px;
    background: ${light ? "#f4f5f7" : "#08090a"}; color: ${light ? "#16181b" : "#f4f5f6"};
    font-family: Geist, system-ui, sans-serif; display: grid; grid-template-columns: 760px 1fr;
    grid-template-rows: auto 1fr; gap: 20px 28px; }
  header { grid-column: 1 / -1; display: flex; align-items: baseline; gap: 16px; }
  h1 { margin: 0; font-size: 30px; font-weight: 600; letter-spacing: -0.01em; }
  header p { margin: 0; font-size: 17px; color: ${light ? "#4b5059" : "#a1a7b0"}; }
  .card { border-radius: 18px; overflow: hidden; background: #08090a;
    border: 1px solid ${light ? "rgb(0 0 0 / 0.08)" : "rgb(255 255 255 / 0.1)"};
    box-shadow: ${light ? "0 18px 40px -18px rgb(20 20 40 / 0.35)" : "none"}; }
  .house { display: grid; place-items: center; padding: 8px; }
  .house img { width: 100%; }
  .right { display: grid; grid-template-rows: auto auto; gap: 20px; align-content: start; }
  .right img { display: block; width: 100%; }
  .alpha img { padding: 12px 8px 0; background: #101113; }
  figcaption { font-family: "Geist Mono", monospace; font-size: 13px; padding: 10px 16px;
    color: #a1a7b0; border-top: 1px solid rgb(255 255 255 / 0.08); }
</style></head><body>
<header><h1>idbd</h1><p>Sutton's IDBD (1992) in Rust, running in the browser through WebAssembly</p></header>
<figure class="card house" style="margin:0"><img src="${png(house)}" alt=""></figure>
<div class="right">
  <figure class="card" style="margin:0"><img src="${png(speeds)}" alt="">
    <figcaption>one fixed speed: late or nervous, measured on the simulated house</figcaption></figure>
  <figure class="card alpha" style="margin:0"><img src="${png(alpha)}" alt="">
    <figcaption>IDBD's step sizes after ${EXPLORER_SAMPLES.toLocaleString("en-US")} samples of Sutton's task</figcaption></figure>
</div>
</body></html>`;
}

async function renderStill(browser, captures) {
  for (const scheme of ["dark", "light"]) {
    const page = await browser.newPage({ viewport: { width: 1600, height: 900 }, deviceScaleFactor: 2 });
    await page.setContent(composite(captures, scheme), { waitUntil: "networkidle" });
    await page.evaluate(() => document.fonts.ready);
    const file = join(work, `hero-${scheme}.png`);
    await page.screenshot({ path: file });
    await page.close();
    execFileSync("cwebp", ["-quiet", "-q", "86", "-resize", "1600", "0", file, "-o", join(assets, `hero-${scheme}.webp`)]);
  }
}

async function renderLoop(browser) {
  const page = await openStory(browser, { viewport: { width: 940, height: 900 }, deviceScaleFactor: 1 });
  await page.clock.install();
  await page.goto(`${BASE}/`);
  await page.clock.runFor(2000);
  await page.waitForSelector('body[data-app-ready="true"]', { timeout: 30000 });
  await page.addStyleTag({ content: LOOP_ONLY });
  await page.evaluate((guess) => {
    for (const [sensor, from, until] of guess) window.idbdStory.family.guess(sensor, from, until);
  }, LOOP_GUESS);
  await page.locator("#play").scrollIntoViewIfNeeded();
  await page.click("#family-run");
  const stage = page.locator("#family-stage");
  const frames = [];
  const shoot = async () => {
    const file = join(work, `frame-${String(frames.length).padStart(3, "0")}.png`);
    await stage.screenshot({ path: file });
    frames.push(file);
  };
  while ((await page.evaluate(() => window.idbdStory.game().phase)) === "running") {
    await shoot();
    await page.clock.runFor(LOOP_CLOCK_MS);
  }
  await page.waitForTimeout(300);
  await shoot();
  await page.close();
  const args = ["-loop", "0", "-lossy", "-q", "70", "-m", "6", "-d", String(LOOP_FRAME_MS)];
  const [first, ...rest] = frames;
  const last = rest.pop();
  execFileSync("img2webp", [
    ...args.slice(0, -1),
    String(LOOP_START_MS),
    first,
    "-d",
    String(LOOP_FRAME_MS),
    ...rest,
    "-d",
    String(LOOP_HOLD_MS),
    last,
    "-o",
    join(assets, "demo-loop.webp"),
  ]);
  return frames.length + 1;
}

try {
  await waitForServer();
  const hero = process.argv.includes("--hero");
  const browser = await chromium.launch();
  if (hero) await renderStill(browser, await captureStill(browser));
  process.stdout.write(`demo-loop.webp: ${await renderLoop(browser)} frames\n`);
  await browser.close();
  const written = hero ? ["hero-dark.webp", "hero-light.webp", "demo-loop.webp"] : ["demo-loop.webp"];
  for (const name of written) {
    const bytes = readFileSync(join(assets, name)).length;
    process.stdout.write(`${name}: ${(bytes / 1024).toFixed(0)} KiB\n`);
  }
} finally {
  server.kill();
  rmSync(work, { recursive: true, force: true });
}
