// Boots the WebAssembly module, checks the crate's reference fixtures in the browser
// and wires up the two sections.

import init, * as wasm from "../pkg/idbd_wasm.js";
import { fillPairs, tooltip } from "./dom.js";
import { createExperiment } from "./experiment.js";
import { createGuided } from "./guided.js";
import { REFERENCE_FIXTURE, withinTolerance } from "./presets.js";

/**
 * Replays scenarios C06 and C07 through the adapter and compares the exposed values
 * with the crate's independently calculated fixture (scenario D01).
 */
function checkReferenceFixture() {
  const { config, samples, expected } = REFERENCE_FIXTURE;
  const learner = new wasm.Learner(config.numFeatures, config.initialStepSize, config.theta);
  try {
    for (const sample of samples) {
      learner.update(new Float64Array(sample.features), sample.target);
    }
    const got = {
      prediction: learner.prediction(),
      error: learner.error(),
      weights: Array.from(learner.weights()),
      betas: Array.from(learner.logStepSizes()),
      traces: Array.from(learner.traces()),
    };
    const checks = [
      withinTolerance(got.prediction, expected.prediction),
      withinTolerance(got.error, expected.error),
      ...expected.weights.map((value, i) => withinTolerance(got.weights[i], value)),
      ...expected.betas.map((value, i) => withinTolerance(got.betas[i], value)),
      ...expected.traces.map((value, i) => withinTolerance(got.traces[i], value)),
    ];
    return { got, expected, matches: checks.every(Boolean) };
  } finally {
    learner.free();
  }
}

async function main() {
  const body = document.body;
  try {
    await init();
  } catch (error) {
    const notice = document.getElementById("load-error");
    notice.hidden = false;
    notice.textContent =
      `The WebAssembly module did not load, so there is nothing to show: ${error}. ` +
      `Build it with "just web-build" and serve this directory over HTTP.`;
    return;
  }

  tooltip.attach();
  const fixture = checkReferenceFixture();

  fillPairs(document.getElementById("build-info"), [
    ["rust core version", wasm.coreVersion()],
    ["random generator", wasm.generatorId()],
    ["inputs per experiment", String(wasm.numFeatures())],
    [
      "reference fixtures C06 and C07",
      fixture.matches ? "match the crate's values in this browser" : "DO NOT MATCH",
    ],
  ]);

  const guided = createGuided(wasm);
  const experiment = createExperiment(wasm);

  // Read-only handle for the browser tests. The page never reads it back.
  window.idbdExplorer = {
    wasm,
    fixture,
    guided: () => guided.snapshot(),
    experiment: () => experiment.snapshot(),
    exportRun: () => experiment.exportRun(),
    isRunning: () => experiment.isRunning(),
  };

  body.dataset.appReady = "true";
}

main();
