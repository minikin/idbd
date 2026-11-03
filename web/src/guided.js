// The guided single update (scenario D14). Every number comes from the Rust
// walkthrough learner, read from the core before and after its own update.

import { clear, el, fillPairs } from "./dom.js";
import { featureName, fmt, fmtAlpha, fmtSigned, signWord } from "./format.js";

const GUIDED_SEED = 7;

export function createGuided(wasm) {
  const nodes = {
    badge: document.getElementById("guided-sample"),
    table: document.querySelector("#guided-sample-table tbody"),
    stateTitle: document.getElementById("guided-state-title"),
    stateTable: document.querySelector("#guided-state-table tbody"),
    pairs: document.getElementById("guided-sample-pairs"),
    choice: document.getElementById("guided-feature-choice"),
    ladder: document.getElementById("guided-ladder"),
    ladderFeature: document.getElementById("ladder-feature"),
    note: document.getElementById("guided-note"),
    advance: document.getElementById("guided-advance"),
    restart: document.getElementById("guided-restart"),
    continueButton: document.getElementById("guided-continue"),
  };

  let walk = new wasm.Walkthrough(GUIDED_SEED);
  let selected = 0;
  let snapshot = null;

  function build() {
    walk = new wasm.Walkthrough(GUIDED_SEED);
    walk.advance();
    render();
  }

  function select(index) {
    selected = index;
    render();
  }

  function renderChoice() {
    clear(nodes.choice);
    for (let index = 0; index < walk.numFeatures(); index += 1) {
      const name = featureName(index);
      const button = el("button", {
        type: "button",
        class: "button",
        role: "radio",
        "aria-checked": index === selected ? "true" : "false",
        "aria-label": `Follow input ${name} through the update`,
        "data-feature": index,
        text: name,
      });
      button.addEventListener("click", () => select(index));
      nodes.choice.append(button);
    }
  }

  function rung(step) {
    const body = el("div", { class: "rung-body" }, [
      el("p", { class: "rung-label", text: step.label }),
      el("p", { class: "rung-value mono", text: step.value }),
    ]);
    if (step.factors) {
      const row = el("div", { class: "factors" });
      for (const factor of step.factors) {
        row.append(el("span", { class: "factor", text: factor }));
      }
      body.append(row);
    }
    if (step.note) {
      body.append(el("p", { class: "rung-note", text: step.note }));
    }
    return el("li", { class: step.gate ? "rung gate" : "rung" }, [body]);
  }

  function render() {
    const sample = walk.sample();
    const name = featureName(selected);
    const features = Array.from(walk.features());
    const coefficients = Array.from(walk.coefficients());
    const coordinate = JSON.parse(walk.coordinateJson(selected));
    const prediction = walk.prediction() ?? 0;
    const error = walk.error() ?? 0;
    const target = walk.target();

    nodes.badge.textContent = `sample ${sample}`;
    nodes.badge.dataset.sample = String(sample);
    nodes.ladderFeature.textContent = name;

    clear(nodes.table);
    for (let index = 0; index < features.length; index += 1) {
      const row = el("tr", { "aria-selected": index === selected ? "true" : "false" }, [
        el("th", { scope: "row", text: featureName(index) }),
        el("td", { text: fmtSigned(features[index], 2) }),
        el("td", { text: fmtSigned(coefficients[index], 1) }),
      ]);
      nodes.table.append(row);
    }

    fillPairs(nodes.pairs, [
      [`target of sample ${sample}`, fmtSigned(target, 4)],
      ["learner", "3 inputs, IDBD"],
      ["initial alpha", fmtAlpha(walk.initialStepSize())],
      ["theta", fmt(walk.theta(), 3)],
    ]);

    renderChoice();

    const product = error * coordinate.input * coordinate.oldTrace;
    const direction =
      product > 0
        ? "up, so alpha grows"
        : product < 0
          ? "down, so alpha shrinks"
          : "nowhere, so alpha is unchanged";
    const zeroTrace = coordinate.oldTrace === 0;
    const steps = [
      {
        label: `Prediction for sample ${sample}, made before learning from it`,
        value: fmtSigned(prediction, 4),
        note: "The weights held after the previous sample, applied to this sample's inputs.",
      },
      {
        label: "Error, also measured before learning: target minus prediction",
        value: fmtSigned(error, 4),
        note: `${fmtSigned(target, 4)} minus ${fmtSigned(prediction, 4)}. Both learners in the experiment below are scored on this pre-update error.`,
      },
      {
        label: `Trace h of ${name} before sample ${sample}`,
        value: fmtSigned(coordinate.oldTrace, 4),
        note: "A running record of how this input's recent weight changes lined up. The beta step below uses this old trace, not the new one.",
      },
      {
        label: "Beta adjustment: theta times error times input times old trace",
        value: fmtSigned(coordinate.betaChange, 6),
        factors: [
          `theta ${fmt(walk.theta(), 3)}`,
          `error ${fmtSigned(error, 4)}`,
          `${name} ${fmtSigned(coordinate.input, 2)}`,
          `old trace ${fmtSigned(coordinate.oldTrace, 4)}`,
        ],
        note: zeroTrace
          ? `The old trace is zero, so the product is zero and beta cannot move on this sample. Theta only scales the step.`
          : `Error is ${signWord(error)}, the input is ${signWord(coordinate.input)} and the old trace is ${signWord(coordinate.oldTrace)}, so their product is ${signWord(product)} and beta moves ${direction}. Theta only scales the step.`,
      },
      {
        gate: true,
        label: `New step size alpha for ${name}, used by both updates below`,
        value: fmtAlpha(coordinate.newAlpha),
        note: `alpha = exp(beta). Before sample ${sample} it was ${fmtAlpha(coordinate.oldAlpha)}; the weight and trace updates that follow use the new value.`,
      },
      {
        label: "Weight update: w plus alpha times error times input",
        value: `${fmtSigned(coordinate.oldWeight, 4)} to ${fmtSigned(coordinate.newWeight, 4)}`,
        note: `Before sample ${sample} to after sample ${sample}, a change of ${fmtSigned(coordinate.weightChange, 4)}.`,
      },
      {
        label: "Trace update: h times max(0, 1 minus alpha x squared), plus alpha times error times input",
        value: `${fmtSigned(coordinate.oldTrace, 4)} to ${fmtSigned(coordinate.newTrace, 4)}`,
        note: `Before sample ${sample} to after sample ${sample}. This trace is what the next sample's beta step will read.`,
      },
    ];

    clear(nodes.ladder);
    for (const step of steps) {
      nodes.ladder.append(rung(step));
    }

    nodes.note.textContent =
      sample === 1
        ? "Sample 1 starts from zero weights and zero traces, so no step size can move yet. Advance again to see beta change."
        : `Showing sample ${sample}. Values labeled "before" are the committed state after sample ${sample - 1}.`;

    const weights = Array.from(walk.weights());
    const stepSizes = Array.from(walk.stepSizes());
    const traces = Array.from(walk.traces());
    nodes.stateTitle.textContent = `State after sample ${sample}`;
    clear(nodes.stateTable);
    for (let index = 0; index < weights.length; index += 1) {
      nodes.stateTable.append(
        el("tr", { "aria-selected": index === selected ? "true" : "false" }, [
          el("th", { scope: "row", text: featureName(index) }),
          el("td", { text: fmtSigned(weights[index], 4) }),
          el("td", { text: fmtAlpha(stepSizes[index]) }),
          el("td", { text: fmtSigned(traces[index], 4) }),
        ]),
      );
    }

    snapshot = {
      sample,
      selected,
      name,
      features,
      coefficients,
      target,
      prediction,
      error,
      coordinate,
      theta: walk.theta(),
      initialStepSize: walk.initialStepSize(),
      seed: GUIDED_SEED,
      weights,
      stepSizes,
      traces,
    };
  }

  nodes.advance.addEventListener("click", () => {
    walk.advance();
    render();
  });
  nodes.restart.addEventListener("click", build);
  nodes.continueButton.addEventListener("click", () => {
    const target = document.getElementById("experiment");
    target.scrollIntoView({ behavior: "smooth", block: "start" });
    document.getElementById("run").focus();
  });

  build();

  return { snapshot: () => snapshot };
}
