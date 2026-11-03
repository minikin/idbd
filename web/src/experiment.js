// The live experiment: one shared stream, IDBD and a fixed-step LMS baseline.
// All learning happens in the Rust session; this file reads committed state and draws.

import { renderAlphaChart, renderErrorChart, renderSparkline, renderTimeline, renderWeightRow, weightRowLabel } from "./charts.js";
import { clear, el, fillLegend, fillPairs, onResize } from "./dom.js";
import { featureName, fmt, fmtAlpha, fmtCount, fmtMse, fmtSigned, signWord } from "./format.js";
import {
  DEFAULT_INITIAL_STEP_SIZE,
  DEFAULT_SEED,
  DEFAULT_SPEED,
  DEFAULT_THETA,
  LMS_NOTE,
  PRESET_COPY,
  DEFAULT_LMS_STEP_SIZE,
  SPEEDS,
  THETA_NOTE,
} from "./presets.js";

/** Chart points per series. Enough to read, small enough to copy every frame. */
const MAX_POINTS = 260;

/** Samples per synchronous batch, so a tick never blocks input for long. */
const BATCH = 256;

/** Milliseconds of work one playback tick may do before it yields. */
const TICK_BUDGET_MS = 8;

/** Minimum milliseconds between redraws while running. */
const RENDER_INTERVAL_MS = 60;

export function createExperiment(wasm) {
  const nodes = {
    run: document.getElementById("run"),
    pause: document.getElementById("pause"),
    step: document.getElementById("step"),
    changeWorld: document.getElementById("change-world"),
    reset: document.getElementById("reset"),
    export: document.getElementById("export"),
    replay: document.getElementById("replay"),
    loadRunButton: document.getElementById("load-run-button"),
    loadRun: document.getElementById("load-run"),
    replayNote: document.getElementById("replay-note"),
    speed: document.getElementById("speed"),
    preset: document.getElementById("preset"),
    presetDescription: document.getElementById("preset-description"),
    seed: document.getElementById("seed"),
    alpha0: document.getElementById("alpha0"),
    theta: document.getElementById("theta"),
    lmsStep: document.getElementById("lms-step"),
    apply: document.getElementById("apply-settings"),
    playbackState: document.getElementById("playback-state"),
    runHeader: document.getElementById("run-header"),
    failure: document.getElementById("failure"),
    badges: Array.from(document.querySelectorAll("#experiment [data-sample-badge]")),
    weightsLegend: document.getElementById("weights-legend"),
    weightList: document.getElementById("weight-list"),
    errorLegend: document.getElementById("error-legend"),
    errorSubtitle: document.getElementById("error-subtitle"),
    errorChart: document.getElementById("error-chart"),
    alphaLegend: document.getElementById("alpha-legend"),
    alphaChart: document.getElementById("alpha-chart"),
    eventsLegend: document.getElementById("events-legend"),
    timeline: document.getElementById("event-timeline"),
    eventSummary: document.getElementById("event-summary"),
    eventList: document.getElementById("event-list"),
    inspectorName: document.getElementById("inspector-name"),
    inspectorPairs: document.getElementById("inspector-pairs"),
    inspectorMeaning: document.getElementById("inspector-meaning"),
    inspectorAlpha: document.getElementById("inspector-alpha"),
    metricsPairs: document.getElementById("metrics-pairs"),
    stateTable: document.querySelector("#state-table tbody"),
  };

  const presets = JSON.parse(wasm.presetsJson());
  const featureCount = wasm.numFeatures();
  const rollingWindow = wasm.rollingWindow();

  let settings = {
    preset: presets[0].id,
    seed: DEFAULT_SEED,
    initialStepSize: DEFAULT_INITIAL_STEP_SIZE,
    theta: DEFAULT_THETA,
    lmsStepSize: DEFAULT_LMS_STEP_SIZE,
  };
  let explorer = null;
  let selected = 0;
  let running = false;
  let speed = DEFAULT_SPEED;
  let frame = null;
  let backlog = 0;
  let lastTime = 0;
  let lastRender = 0;
  let state = null;
  let rows = [];
  // A replay repeats a recorded run: the same settings, the recorded "Change the world"
  // samples, and a pause at the recorded sample count. Null for an ordinary run.
  let replay = null;
  let loadError = null;

  /* ---------- setup ---------- */

  for (const option of SPEEDS) {
    nodes.speed.append(el("option", { value: option.value, text: option.label }));
  }
  nodes.speed.value = String(DEFAULT_SPEED);

  for (const preset of presets) {
    const copy = PRESET_COPY[preset.id];
    nodes.preset.append(el("option", { value: preset.id, text: copy ? copy.name : preset.id }));
  }
  nodes.preset.value = settings.preset;

  nodes.seed.value = String(settings.seed);
  nodes.alpha0.value = String(settings.initialStepSize);
  nodes.theta.value = String(settings.theta);
  nodes.lmsStep.value = String(settings.lmsStepSize);

  fillLegend(nodes.weightsLegend, [
    { label: "IDBD weight", color: "var(--idbd)", shape: "bar" },
    { label: "LMS weight", color: "var(--lms)", shape: "bar" },
    { label: "true coefficient (simulator)", color: "var(--truth)", shape: "outline" },
  ]);
  fillLegend(nodes.errorLegend, [
    { label: "IDBD rolling MSE", color: "var(--idbd)", shape: "line" },
    { label: "LMS rolling MSE", color: "var(--lms)", shape: "line" },
    { label: "Autostep rolling MSE, untuned (AUTO)", color: "var(--autostep)", shape: "dashed" },
    { label: "relevance shift", color: "var(--truth)", shape: "diamond" },
  ]);
  fillLegend(nodes.eventsLegend, [
    { label: "sign flip (tick)", color: "var(--truth)", shape: "line" },
    { label: "relevance shift (flag)", color: "var(--truth)", shape: "diamond" },
  ]);
  fillLegend(nodes.alphaLegend, [
    { label: "selected input", color: "var(--idbd)", shape: "line" },
    { label: "currently relevant input", color: "var(--ink-3)", shape: "line" },
    { label: "irrelevant input", color: "var(--ink-3)", shape: "dashed" },
  ]);
  nodes.errorSubtitle.textContent =
    `Rolling mean squared error of the pre-update prediction error over the last ` +
    `min(${rollingWindow}, completed) samples, for both learners.`;

  buildWeightRows();
  describePreset();
  build();

  let redrawPending = false;
  function redraw() {
    if (redrawPending) {
      return;
    }
    redrawPending = true;
    requestAnimationFrame(() => {
      redrawPending = false;
      draw();
    });
  }
  onResize(nodes.errorChart, redraw);
  window.addEventListener("resize", redraw);

  /* ---------- session lifecycle ---------- */

  function build(plan = null) {
    stop();
    const next = new wasm.Explorer(
      settings.preset,
      settings.seed,
      settings.initialStepSize,
      settings.theta,
      settings.lmsStepSize,
    );
    try {
      for (const sample of plan ? plan.changes : []) {
        next.scheduleChange(sample);
      }
    } catch (error) {
      next.free();
      throw error;
    }
    if (explorer) {
      explorer.free();
    }
    explorer = next;
    replay = plan ? { ...plan, reached: false } : null;
    loadError = null;
    backlog = 0;
    if (!endReplayIfReached()) {
      render();
    }
  }

  /** Pauses a replay at its recorded sample count. Returns true if it paused. */
  function endReplayIfReached() {
    if (!replay || replay.reached || explorer.completed() < replay.stopAt) {
      return false;
    }
    replay = { ...replay, reached: true };
    backlog = 0;
    stop();
    render();
    return true;
  }

  function describePreset() {
    const copy = PRESET_COPY[nodes.preset.value];
    nodes.presetDescription.textContent = copy ? copy.description : "";
  }

  function readSettings() {
    const seed = Number(nodes.seed.value);
    const initialStepSize = Number(nodes.alpha0.value);
    const theta = Number(nodes.theta.value);
    const lmsStepSize = Number(nodes.lmsStep.value);
    return {
      preset: nodes.preset.value,
      seed: Number.isInteger(seed) && seed >= 0 ? seed : settings.seed,
      initialStepSize: initialStepSize > 0 ? initialStepSize : settings.initialStepSize,
      theta: theta >= 0 ? theta : settings.theta,
      lmsStepSize: lmsStepSize > 0 ? lmsStepSize : settings.lmsStepSize,
    };
  }

  /* ---------- playback ---------- */

  function start() {
    if (running || (explorer && explorer.hasFailed())) {
      return;
    }
    running = true;
    lastTime = performance.now();
    lastRender = 0;
    frame = requestAnimationFrame(tick);
    renderPlaybackState();
  }

  function stop() {
    running = false;
    if (frame !== null) {
      cancelAnimationFrame(frame);
      frame = null;
    }
    renderPlaybackState();
  }

  function tick(now) {
    if (!running) {
      return;
    }
    const elapsed = Math.min(now - lastTime, 250);
    lastTime = now;
    // The sample schedule follows wall-clock time, not the frame rate.
    backlog = Math.min(backlog + (elapsed / 1000) * speed, speed);
    let due = Math.floor(backlog);
    if (replay && !replay.reached) {
      due = Math.min(due, replay.stopAt - explorer.completed());
    }
    const started = performance.now();
    let ok = true;
    while (due > 0 && ok && performance.now() - started < TICK_BUDGET_MS) {
      const batch = Math.min(due, BATCH);
      ok = advance(batch);
      if (ok) {
        due -= batch;
        backlog -= batch;
      }
    }
    if (endReplayIfReached()) {
      return;
    }
    if (now - lastRender >= RENDER_INTERVAL_MS) {
      lastRender = now;
      render();
    }
    if (running) {
      frame = requestAnimationFrame(tick);
    }
  }

  function advance(samples) {
    try {
      explorer.run(samples);
    } catch {
      onFailure();
      return false;
    }
    if (explorer.hasFailed()) {
      onFailure();
      return false;
    }
    return true;
  }

  function onFailure() {
    stop();
    render();
    nodes.reset.focus();
  }

  /* ---------- state ---------- */

  function readState() {
    const samples = Array.from(explorer.historySamples(MAX_POINTS));
    const coefficients = Array.from(explorer.coefficients());
    const alphaSeries = [];
    for (let index = 0; index < featureCount; index += 1) {
      alphaSeries.push(Array.from(explorer.alphaHistory(index, MAX_POINTS)));
    }
    const failed = explorer.hasFailed();
    return {
      sample: explorer.completed(),
      preset: explorer.preset(),
      presetFromPaper: explorer.presetFromPaper(),
      seed: explorer.seed(),
      initialStepSize: explorer.initialStepSize(),
      theta: explorer.theta(),
      lmsStepSize: explorer.lmsStepSize(),
      features: Array.from(explorer.features()),
      coefficients,
      relevant: coefficients.map((value) => value !== 0),
      idbdWeights: Array.from(explorer.idbdWeights()),
      lmsWeights: Array.from(explorer.lmsWeights()),
      autostepWeights: Array.from(explorer.autostepWeights()),
      autostepAlphas: Array.from(explorer.autostepStepSizes()),
      autostepConfig: JSON.parse(explorer.autostepConfigJson()),
      alphas: Array.from(explorer.idbdStepSizes()),
      traces: Array.from(explorer.idbdTraces()),
      target: explorer.target(),
      idbdPrediction: explorer.idbdPrediction() ?? null,
      idbdError: explorer.idbdError() ?? null,
      lmsError: explorer.lmsError() ?? null,
      autostepError: explorer.autostepError() ?? null,
      idbdRollingMse: explorer.idbdRollingMse() ?? null,
      lmsRollingMse: explorer.lmsRollingMse() ?? null,
      autostepRollingMse: explorer.autostepRollingMse() ?? null,
      idbdMse: explorer.idbdMeanSquaredError(),
      lmsMse: explorer.lmsMeanSquaredError(),
      autostepMse: explorer.autostepMeanSquaredError(),
      historyLength: explorer.historyLength(),
      samples,
      idbdRollingSeries: Array.from(explorer.historyIdbdRollingMse(MAX_POINTS)),
      lmsRollingSeries: Array.from(explorer.historyLmsRollingMse(MAX_POINTS)),
      autostepRollingSeries: Array.from(explorer.historyAutostepRollingMse(MAX_POINTS)),
      alphaSeries,
      events: JSON.parse(explorer.eventsJson()),
      requestedChanges: Array.from(explorer.requestedChanges()),
      scheduledChanges: Array.from(explorer.scheduledChanges()),
      replay,
      changePending: explorer.changePending(),
      coordinate: explorer.completed() > 0 ? JSON.parse(explorer.coordinateJson(selected)) : null,
      failure: failed
        ? {
            learner: explorer.failureLearner(),
            sample: explorer.failureSample(),
            cause: explorer.failureCause(),
            message: explorer.failureMessage(),
          }
        : null,
    };
  }

  /* ---------- rendering ---------- */

  function render() {
    state = readState();
    for (const badge of nodes.badges) {
      badge.textContent = `sample ${fmtCount(state.sample)}`;
      badge.dataset.sample = String(state.sample);
    }
    renderRunHeader();
    renderFailure();
    renderWeights();
    renderInspector();
    renderMetrics();
    renderTable();
    renderEvents();
    renderPlaybackState();
    renderReplayNote();
    draw();
  }

  function renderRunHeader() {
    const copy = PRESET_COPY[state.preset];
    fillPairs(nodes.runHeader, [
      ["task", copy ? copy.name : state.preset],
      ["task origin", state.presetFromPaper ? "Sutton (1992)" : "extension, not from the paper"],
      ["seed", String(state.seed)],
      ["rust core version", wasm.coreVersion()],
      ["generator", wasm.generatorId()],
      [
        "idbd",
        `${featureCount} inputs, alpha0 ${fmtAlpha(state.initialStepSize)}, theta ${fmt(state.theta, 4)}, no bounds`,
      ],
      ["theta note", THETA_NOTE, "wide-pair"],
      [
        "lms baseline",
        `fixed-step LMS, ${featureCount} inputs, alpha ${fmt(state.lmsStepSize, 4)}`,
      ],
      ["lms note", LMS_NOTE, "wide-pair"],
      [
        "autostep",
        `${featureCount} inputs, alpha0 ${fmtAlpha(state.autostepConfig.initialStepSize)}, ` +
          `mu ${fmt(state.autostepConfig.metaStepSize, 4)}, tau ${fmtCount(state.autostepConfig.timeScale)}: ` +
          `the paper's defaults, not tuned to this task`,
      ],
      ["rolling window", `${rollingWindow} samples`],
      ["data", "synthetic, generated in the browser"],
    ]);
  }

  function renderFailure() {
    const failed = state.failure !== null;
    nodes.failure.hidden = !failed;
    clear(nodes.failure);
    if (failed) {
      nodes.failure.append(
        el("p", {}, [
          el("strong", { text: `${state.failure.learner} failed on sample ${fmtCount(state.failure.sample)}.` }),
          ` ${state.failure.cause}. Neither learner learned from that sample, so every panel below stays at sample ${fmtCount(state.sample)}, the last one both completed. Choose Reset to start a fresh run.`,
        ]),
      );
    }
    for (const button of [nodes.run, nodes.step, nodes.changeWorld]) {
      button.disabled = failed;
    }
    nodes.replay.disabled = state.sample === 0;
  }

  function renderReplayNote() {
    let text = null;
    if (loadError) {
      text = `Could not load the run: ${loadError}.`;
    } else if (replay) {
      const changes = replay.changes.length;
      const what = `${replay.source}, ${fmtCount(changes)} recorded world ${changes === 1 ? "change" : "changes"}`;
      text = replay.reached
        ? `Replay of ${what}. Paused at sample ${fmtCount(replay.stopAt)}, the recorded sample count. Run continues past it.`
        : `Replay of ${what}. Playback pauses at sample ${fmtCount(replay.stopAt)}, at any speed.`;
      if (!replay.sameBuild) {
        text += " The file comes from a different build, so the results can differ.";
      }
    }
    nodes.replayNote.hidden = text === null;
    nodes.replayNote.classList.toggle("error", loadError !== null);
    nodes.replayNote.textContent = text ?? "";
  }

  function renderPlaybackState() {
    if (!state) {
      return;
    }
    let text;
    if (state.failure) {
      text = "Stopped by a numerical failure. Reset to continue.";
    } else if (running) {
      text = `Running at ${fmtCount(speed)} samples / second`;
    } else {
      text = "Paused";
    }
    if (state.changePending) {
      text += " · world change applies to the next sample";
    }
    nodes.playbackState.textContent = text;
  }

  function buildWeightRows() {
    clear(nodes.weightList);
    rows = [];
    for (let index = 0; index < featureCount; index += 1) {
      const name = el("span", { class: "name", text: featureName(index) });
      const bars = el("span", { class: "bars" });
      const row = el(
        "div",
        {
          class: "weight-row",
          role: "option",
          id: `weight-row-${index}`,
          tabindex: index === 0 ? "0" : "-1",
          "aria-selected": index === 0 ? "true" : "false",
        },
        [name, bars],
      );
      row.addEventListener("click", () => selectFeature(index));
      row.addEventListener("keydown", (event) => onRowKey(event, index));
      nodes.weightList.append(row);
      rows.push({ row, bars });
    }
    nodes.weightList.setAttribute("aria-activedescendant", "weight-row-0");
  }

  function onRowKey(event, index) {
    const moves = {
      ArrowDown: index + 1,
      ArrowRight: index + 1,
      ArrowUp: index - 1,
      ArrowLeft: index - 1,
      Home: 0,
      End: featureCount - 1,
    };
    if (event.key in moves) {
      event.preventDefault();
      const next = Math.max(0, Math.min(featureCount - 1, moves[event.key]));
      selectFeature(next);
      rows[next].row.focus();
    } else if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      selectFeature(index);
    }
  }

  function selectFeature(index) {
    selected = index;
    nodes.weightList.setAttribute("aria-activedescendant", `weight-row-${index}`);
    for (let i = 0; i < rows.length; i += 1) {
      rows[i].row.setAttribute("aria-selected", i === index ? "true" : "false");
      rows[i].row.tabIndex = i === index ? 0 : -1;
    }
    render();
  }

  function renderWeights() {
    const scale = Math.max(
      0.25,
      ...state.idbdWeights.map(Math.abs),
      ...state.lmsWeights.map(Math.abs),
      ...state.coefficients.map(Math.abs),
    );
    for (let index = 0; index < featureCount; index += 1) {
      const values = {
        idbd: state.idbdWeights[index],
        lms: state.lmsWeights[index],
        truth: state.coefficients[index],
        alpha: state.alphas[index],
      };
      renderWeightRow(rows[index].bars, { ...values, scale });
      rows[index].row.setAttribute("aria-label", weightRowLabel(featureName(index), values));
    }
  }

  function renderInspector() {
    const name = featureName(selected);
    nodes.inspectorName.textContent = name;
    const coordinate = state.coordinate;
    fillPairs(nodes.inspectorPairs, [
      [`input value at sample ${fmtCount(state.sample)}`, fmtSigned(state.features[selected], 4)],
      ["true coefficient (known simulator information)", fmtSigned(state.coefficients[selected], 3)],
      ["idbd weight", fmtSigned(state.idbdWeights[selected], 5)],
      ["lms weight", fmtSigned(state.lmsWeights[selected], 5)],
      ["autostep weight", fmtSigned(state.autostepWeights[selected], 5)],
      ["idbd alpha (size of the learning adjustment)", fmtAlpha(state.alphas[selected])],
      ["autostep alpha", fmtAlpha(state.autostepAlphas[selected])],
      ["idbd trace h", fmtSigned(state.traces[selected], 5)],
      ["last beta change", coordinate ? fmtSigned(coordinate.betaChange, 6) : "no sample yet"],
      ["last weight change", coordinate ? fmtSigned(coordinate.weightChange, 6) : "no sample yet"],
    ]);

    if (!coordinate) {
      nodes.inspectorMeaning.textContent =
        "Advance at least one sample to see what the last adjustment did.";
    } else {
      const product = state.idbdError * coordinate.input * coordinate.oldTrace;
      const direction =
        coordinate.betaChange > 0
          ? "grew"
          : coordinate.betaChange < 0
            ? "shrank"
            : "did not move";
      nodes.inspectorMeaning.textContent =
        `On sample ${fmtCount(state.sample)} the step size for ${name} ${direction}: ` +
        `${fmtAlpha(coordinate.oldAlpha)} to ${fmtAlpha(coordinate.newAlpha)}. ` +
        `Error times input times old trace was ${signWord(product)}, which is the sign the beta step follows. ` +
        `Alpha is the size of the learning adjustment this input receives on the next correction, not its importance.`;
    }
  }

  function renderMetrics() {
    fillPairs(nodes.metricsPairs, [
      ["completed samples", fmtCount(state.sample)],
      ["retained for charts", `${fmtCount(state.historyLength)} of ${fmtCount(wasm.historyCapacity())}`],
      [`idbd rolling mse (last ${rollingWindow})`, fmtMse(state.idbdRollingMse)],
      [`lms rolling mse (last ${rollingWindow})`, fmtMse(state.lmsRollingMse)],
      [`autostep rolling mse (last ${rollingWindow})`, fmtMse(state.autostepRollingMse)],
      ["idbd mse, all samples", fmtMse(state.idbdMse)],
      ["lms mse, all samples", fmtMse(state.lmsMse)],
      ["autostep mse, all samples", fmtMse(state.autostepMse)],
      [`target of sample ${fmtCount(state.sample)}`, fmtSigned(state.target, 5)],
      ["idbd prediction before learning", fmtSigned(state.idbdPrediction, 5)],
      ["idbd pre-update error", fmtSigned(state.idbdError, 5)],
      ["lms pre-update error", fmtSigned(state.lmsError, 5)],
      ["autostep pre-update error", fmtSigned(state.autostepError, 5)],
    ]);
  }

  function renderTable() {
    clear(nodes.stateTable);
    for (let index = 0; index < featureCount; index += 1) {
      nodes.stateTable.append(
        el("tr", { "aria-selected": index === selected ? "true" : "false", "data-feature": index }, [
          el("th", { scope: "row", text: featureName(index) }),
          el("td", { text: fmtSigned(state.features[index], 4) }),
          el("td", { text: fmtSigned(state.coefficients[index], 3) }),
          el("td", { text: fmtSigned(state.idbdWeights[index], 5) }),
          el("td", { text: fmtSigned(state.lmsWeights[index], 5) }),
          el("td", { text: fmtAlpha(state.alphas[index]) }),
          el("td", { text: fmtSigned(state.traces[index], 5) }),
          el("td", { text: fmtSigned(state.autostepWeights[index], 5) }),
          el("td", { text: fmtAlpha(state.autostepAlphas[index]) }),
        ]),
      );
    }
  }

  function renderEvents() {
    const flips = state.events.filter((event) => event.kind === "signFlip").length;
    const shifts = state.events.length - flips;
    nodes.eventSummary.textContent =
      `${fmtCount(flips)} sign flips and ${fmtCount(shifts)} relevance shifts in the ` +
      `retained window. Only relevance shifts are marked on the charts above.`;
    clear(nodes.eventList);
    const recent = state.events.slice(-12).reverse();
    if (!recent.length) {
      nodes.eventList.append(el("li", { text: "No events yet." }));
    }
    for (const event of recent) {
      nodes.eventList.append(el("li", { text: eventText(event) }));
    }
  }

  function eventText(event) {
    if (event.kind === "signFlip") {
      return `sample ${fmtCount(event.sample)}: coefficient of ${featureName(event.feature)} switched sign`;
    }
    const origin = event.origin === "requested" ? "you changed the world" : "scheduled";
    return `sample ${fmtCount(event.sample)}: relevance moved from inputs ${event.from * 5}-${event.from * 5 + 4} to ${event.to * 5}-${event.to * 5 + 4} (${origin})`;
  }

  function draw() {
    if (!state) {
      return;
    }
    renderErrorChart(nodes.errorChart, {
      samples: state.samples,
      idbd: state.idbdRollingSeries,
      lms: state.lmsRollingSeries,
      autostep: state.autostepRollingSeries,
      events: state.events,
      label: `Rolling mean squared error over the last ${rollingWindow} samples for IDBD, the LMS baseline and untuned Autostep`,
    });
    renderAlphaChart(nodes.alphaChart, {
      samples: state.samples,
      series: state.alphaSeries,
      relevant: state.relevant,
      selected,
      events: state.events,
    });
    renderTimeline(nodes.timeline, { samples: state.samples, events: state.events });
    renderSparkline(nodes.inspectorAlpha, {
      samples: state.samples,
      values: state.alphaSeries[selected],
      name: featureName(selected),
    });
  }

  /* ---------- export (D13) ---------- */

  /** The run definition and its results, enough to repeat the run on this build. */
  function exportRun() {
    return {
      kind: "idbd-explorer-run",
      formatVersion: 1,
      data: "synthetic, generated in the browser by idbd-lab",
      build: {
        rustCoreVersion: wasm.coreVersion(),
        generator: wasm.generatorId(),
        reproduce: [
          "git clone https://github.com/minikin/idbd",
          "just web-build",
          "just web-serve",
          "then set the task, seed and settings below and run to the sample count below",
        ],
      },
      settings: {
        preset: state.preset,
        presetFromPaper: state.presetFromPaper,
        seed: state.seed,
        idbd: {
          numFeatures: featureCount,
          initialStepSize: state.initialStepSize,
          theta: state.theta,
          bounds: "none",
        },
        lms: { numFeatures: featureCount, stepSize: state.lmsStepSize, selection: LMS_NOTE },
        autostep: {
          numFeatures: featureCount,
          ...state.autostepConfig,
          selection: "the paper's defaults, not tuned to this task",
        },
        requestedChanges: state.requestedChanges,
      },
      samples: {
        completed: state.sample,
        retainedForCharts: state.historyLength,
        retainedWindow:
          state.samples.length > 0
            ? { first: state.samples[0], last: state.samples[state.samples.length - 1] }
            : null,
        rollingWindow,
      },
      metrics: {
        idbdRollingMse: state.idbdRollingMse,
        lmsRollingMse: state.lmsRollingMse,
        autostepRollingMse: state.autostepRollingMse,
        idbdMeanSquaredError: state.idbdMse,
        lmsMeanSquaredError: state.lmsMse,
        autostepMeanSquaredError: state.autostepMse,
        idbdWeights: state.idbdWeights,
        lmsWeights: state.lmsWeights,
        autostepWeights: state.autostepWeights,
        autostepStepSizes: state.autostepAlphas,
        idbdStepSizes: state.alphas,
        trueCoefficients: state.coefficients,
      },
      events: state.events,
      failure: state.failure,
    };
  }

  function downloadRun() {
    const body = JSON.stringify(exportRun(), null, 2);
    const url = URL.createObjectURL(new Blob([body], { type: "application/json" }));
    const link = el("a", {
      href: url,
      download: `idbd-run-${state.preset}-seed${state.seed}-sample${state.sample}.json`,
    });
    document.body.append(link);
    link.click();
    link.remove();
    URL.revokeObjectURL(url);
  }

  /* ---------- replay (D06) ---------- */

  /** The plan that repeats the current run up to its completed sample. */
  function replayCurrentRun() {
    return {
      source: "this run",
      stopAt: state.sample,
      changes: state.requestedChanges,
      sameBuild: true,
    };
  }

  /** Reads an exported run. Throws an Error whose message says what is wrong. */
  function parseRun(text, name) {
    let run;
    try {
      run = JSON.parse(text);
    } catch {
      throw new Error("the file is not JSON");
    }
    if (run?.kind !== "idbd-explorer-run" || run.formatVersion !== 1) {
      throw new Error("the file is not an IDBD Explorer run export");
    }
    const recorded = run.settings ?? {};
    if (!presets.some((preset) => preset.id === recorded.preset)) {
      throw new Error(`the task "${recorded.preset}" is not available`);
    }
    const positive = (value) => Number.isFinite(value) && value > 0;
    const loaded = {
      preset: recorded.preset,
      seed: recorded.seed,
      initialStepSize: recorded.idbd?.initialStepSize,
      theta: recorded.idbd?.theta,
      lmsStepSize: recorded.lms?.stepSize,
    };
    if (!Number.isSafeInteger(loaded.seed) || loaded.seed < 0) {
      throw new Error("the seed is not a whole number of 0 or more");
    }
    if (!positive(loaded.initialStepSize) || !positive(loaded.lmsStepSize)) {
      throw new Error("a step size is not a positive number");
    }
    if (!Number.isFinite(loaded.theta) || loaded.theta < 0) {
      throw new Error("theta is not a number of 0 or more");
    }
    const stopAt = run.samples?.completed;
    if (!Number.isSafeInteger(stopAt) || stopAt < 0) {
      throw new Error("the completed sample count is missing");
    }
    const changes = recorded.requestedChanges;
    const valid =
      Array.isArray(changes) &&
      changes.every(
        (sample, index) =>
          Number.isSafeInteger(sample) &&
          sample >= 1 &&
          sample <= stopAt &&
          (index === 0 || sample > changes[index - 1]),
      );
    if (!valid) {
      throw new Error("the recorded world changes are not increasing sample indices within the run");
    }
    return {
      settings: loaded,
      plan: {
        source: name,
        stopAt,
        changes,
        sameBuild:
          run.build?.rustCoreVersion === wasm.coreVersion() &&
          run.build?.generator === wasm.generatorId(),
      },
    };
  }

  function showSettings() {
    nodes.preset.value = settings.preset;
    nodes.seed.value = String(settings.seed);
    nodes.alpha0.value = String(settings.initialStepSize);
    nodes.theta.value = String(settings.theta);
    nodes.lmsStep.value = String(settings.lmsStepSize);
    describePreset();
  }

  async function loadRunFile(file) {
    const previous = settings;
    try {
      const { settings: loaded, plan } = parseRun(await file.text(), file.name);
      settings = loaded;
      build(plan);
    } catch (error) {
      settings = previous;
      loadError = error instanceof Error ? error.message : String(error);
      renderReplayNote();
    }
    showSettings();
  }

  /* ---------- controls ---------- */

  nodes.run.addEventListener("click", start);
  nodes.pause.addEventListener("click", () => {
    stop();
    render();
  });
  nodes.step.addEventListener("click", () => {
    stop();
    if (advance(1) && !endReplayIfReached()) {
      render();
    }
  });
  nodes.changeWorld.addEventListener("click", () => {
    explorer.changeTheWorld();
    render();
  });
  nodes.reset.addEventListener("click", () => {
    build();
  });
  nodes.export.addEventListener("click", downloadRun);
  nodes.replay.addEventListener("click", () => {
    build(replayCurrentRun());
  });
  nodes.loadRunButton.addEventListener("click", () => {
    nodes.loadRun.click();
  });
  nodes.loadRun.addEventListener("change", () => {
    const [file] = nodes.loadRun.files;
    nodes.loadRun.value = "";
    if (file) {
      loadRunFile(file);
    }
  });
  nodes.speed.addEventListener("change", () => {
    speed = Number(nodes.speed.value);
    backlog = 0;
    renderPlaybackState();
  });
  nodes.preset.addEventListener("change", () => {
    describePreset();
    settings = { ...settings, preset: nodes.preset.value };
    build();
  });
  nodes.apply.addEventListener("click", () => {
    settings = readSettings();
    showSettings();
    build();
  });

  return {
    snapshot: () => state,
    exportRun,
    isRunning: () => running,
  };
}
