// Shared helpers for the browser scenarios.

/** The acceptance tolerance from features/idbd.feature. */
export function withinTolerance(a, b) {
  return Math.abs(a - b) <= 1e-12 + 1e-10 * Math.max(Math.abs(a), Math.abs(b));
}

/** Loads the demo and waits until the WebAssembly module has booted. */
export async function openApp(page) {
  await page.goto("/");
  await page.waitForSelector('body[data-app-ready="true"]', { timeout: 20000 });
}

/** The sample index every experiment panel is showing. */
export async function experimentSamples(page) {
  return page.$$eval("#experiment [data-sample-badge]", (nodes) =>
    nodes.map((node) => node.dataset.sample),
  );
}

/** The last sample both learners completed. */
export async function completedSample(page) {
  return page.evaluate(() => window.idbdExplorer.experiment().sample);
}

/** The state object the panels were last rendered from. */
export async function experimentState(page) {
  return page.evaluate(() => {
    const state = window.idbdExplorer.experiment();
    return {
      sample: state.sample,
      features: state.features,
      coefficients: state.coefficients,
      idbdWeights: state.idbdWeights,
      lmsWeights: state.lmsWeights,
      autostepWeights: state.autostepWeights,
      autostepAlphas: state.autostepAlphas,
      autostepRollingMse: state.autostepRollingMse,
      alphas: state.alphas,
      traces: state.traces,
      idbdRollingMse: state.idbdRollingMse,
      lmsRollingMse: state.lmsRollingMse,
      events: state.events,
      failure: state.failure,
    };
  });
}

/** Runs the experiment until it has completed at least `samples` samples. */
export async function runUntil(page, samples, speed = "1000") {
  await page.selectOption("#speed", speed);
  await page.click("#run");
  await page.waitForFunction(
    (wanted) => window.idbdExplorer.experiment().sample >= wanted,
    samples,
    { timeout: 30000 },
  );
  await page.click("#pause");
  await page.waitForFunction(() => window.idbdExplorer.isRunning() === false);
}
