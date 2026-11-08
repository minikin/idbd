// Boots the WebAssembly module and fills the story page. Every learning value on the
// page comes from the crate's Family Assistant home: the hero's rings and live chart run
// on FamilyWatch, chapter 1 shows its readings, chapter 2 traces two fixed speeds with
// FamilyDay, and the slow motion, the game and the closing faders use FamilyDay too.

import init, * as wasm from "../pkg/idbd_wasm.js";
import { el, svg } from "../src/dom.js";
import { fmtSigned } from "../src/format.js";
import { SENSOR_COLORS, createHouse2d } from "./house.js";
import { createFamily } from "./family.js";
import { createSlowMotion } from "./slow.js";

const HERO_SEED = 1;
const HERO_TICK_MS = 150;
const SPARK_POINTS = 60;
const GRID_POINTS = 25;

// Chapter 2 traces two fixed speeds through one stretch of day 2 on this seed: from half an
// hour before a sensor with small readings starts to matter until half an hour after it
// stops. The late chart follows that sensor at the tuned fixed speed. The nervous chart
// follows a sensor with large readings that never matters, at a faster fixed speed.
const TRACE_SEED = 1;
const TRACE_DAY = 1;
const TRACE_MARGIN = 30;
const LATE_SENSOR = 1;
const NERVOUS_SENSOR = 2;
const NERVOUS_SPEED = 0.05;

const $ = (id) => document.getElementById(id);
const count = (n) => n.toLocaleString("en-US");
const reducedMotion = window.matchMedia("(prefers-reduced-motion: reduce)");

function points(values, width, height, range) {
  const step = width / Math.max(1, values.length - 1);
  return values
    .map((v, i) => {
      const y = height / 2 - (Math.max(-range, Math.min(range, v)) / range) * (height / 2 - 2);
      return `${(i * step).toFixed(1)},${y.toFixed(1)}`;
    })
    .join(" ");
}

function fillHouseKey(home) {
  const key = $("house-key");
  home.sensors.forEach((sensor, i) => {
    const dot = el("i", { class: "sensor-dot", "aria-hidden": "true" });
    dot.style.background = SENSOR_COLORS[i];
    key.append(el("li", {}, [dot, sensor.name]));
  });
}

/**
 * Draws the house in 3D when WebGL works and motion is welcome, else in 2D. `flatten`
 * switches a running 3D house to 2D, for a lost WebGL context or a reduced-motion
 * preference turned on later.
 */
async function mountRenderer(home) {
  const names = home.sensors.map((s) => s.name);
  const node = $("house");
  const update2d = createHouse2d($("house-2d-sensors"), names);
  const renderer = { kind: "2d", update: update2d, flatten() {} };
  node.dataset.renderer = "2d";
  if (reducedMotion.matches) {
    return renderer;
  }
  try {
    const { mountHouse3d } = await import("./house3d.js");
    let latest = null;
    const scales = home.sensors.map((s) => s.scale);
    const view = mountHouse3d($("house-3d"), names, () => renderer.flatten(), scales);
    const canvas = $("house-3d");
    canvas.hidden = false;
    canvas.setAttribute("role", "img");
    canvas.setAttribute(
      "aria-label",
      "A cutaway home with twelve sensors. An amber ring around each sensor grows with the learning speed IDBD has given it.",
    );
    $("house-2d").style.visibility = "hidden";
    $("house-2d").setAttribute("aria-hidden", "true");
    Object.assign(renderer, {
      kind: "3d",
      update(state) {
        latest = state;
        view.update(state);
      },
      flatten() {
        if (renderer.kind !== "3d") return;
        view.dispose();
        canvas.hidden = true;
        canvas.removeAttribute("role");
        canvas.removeAttribute("aria-label");
        $("house-2d").style.visibility = "";
        $("house-2d").removeAttribute("aria-hidden");
        Object.assign(renderer, { kind: "2d", update: update2d });
        node.dataset.renderer = "2d";
        if (latest) update2d(latest);
        console.warn("The 3D house stopped, so the page draws it in 2D.");
      },
    });
    node.dataset.renderer = "3d";
  } catch (error) {
    console.warn("The 3D house is unavailable, so the page draws it in 2D.", error);
  }
  return renderer;
}

/** "day 8, 14:05" for a count of one-minute readings. */
function clockText(completed) {
  const minute = Math.max(0, completed - 1);
  const day = Math.floor(minute / 1440) + 1;
  const time = minute % 1440;
  return `day ${day}, ${String(Math.floor(time / 60)).padStart(2, "0")}:${String(time % 60).padStart(2, "0")}`;
}

function startHero(home, renderer) {
  const watch = new wasm.FamilyWatch(HERO_SEED);
  const warmup = home.heroWarmupDays * home.hours * home.readingsPerHour;
  watch.run(warmup);
  const readings = home.sensors.map(() => []);
  const actual = [];
  const guess = [];

  const sensors = home.sensors.map((sensor, i) => {
    const dot = el("i", { class: "sensor-dot" });
    dot.style.background = SENSOR_COLORS[i];
    const reading = el("span", { class: "sr-only" });
    const head = el("div", { class: "name" }, [
      el("span", {}, [dot, sensor.name, reading]),
      el("span", { class: "unknown", "aria-hidden": "true", text: "?" }),
    ]);
    const line = svg("polyline", { points: "" });
    const chart = svg("svg", { viewBox: "0 0 240 40", "aria-hidden": "true" }, [line]);
    $("sensor-grid").append(el("li", {}, [head, chart]));
    return { line, reading };
  });

  function record() {
    watch.run(1);
    const x = watch.features();
    readings.forEach((list, i) => {
      list.push(x[i]);
      if (list.length > GRID_POINTS) list.shift();
    });
    actual.push(watch.target());
    guess.push(watch.prediction());
    if (actual.length > SPARK_POINTS) {
      actual.shift();
      guess.shift();
    }
  }

  function draw() {
    const features = Array.from(watch.features());
    renderer.update({ speeds: Array.from(watch.speeds()), features });
    const when = clockText(watch.completed());
    $("live-count").textContent = when;
    $("live-actual").setAttribute("points", points(actual, 240, 56, 4));
    $("live-idbd").setAttribute("points", points(guess, 240, 56, 4));
    $("live-text").textContent =
      `${when[0].toUpperCase()}${when.slice(1)}: the meter read ${fmtSigned(actual.at(-1), 2)} kW against the base, ` +
      `IDBD guessed ${fmtSigned(guess.at(-1), 2)} kW.`;
    sensors.forEach(({ line, reading }, i) => {
      line.setAttribute("points", points(readings[i], 240, 40, 5));
      reading.textContent = `, latest reading ${fmtSigned(features[i], 2)}`;
    });
  }

  for (let i = 0; i < SPARK_POINTS; i += 1) record();
  draw();

  // The loop runs only while the house or the sensor grid is on screen, the tab is shown
  // and the visitor has not paused it. Reduced motion starts paused on a still frame.
  const button = $("hero-pause");
  const shown = new Set();
  let paused = reducedMotion.matches;
  let timer = null;
  function sync() {
    const run = shown.size > 0 && !paused && !document.hidden;
    if (run && timer === null) {
      timer = setInterval(() => {
        record();
        draw();
      }, HERO_TICK_MS);
    } else if (!run && timer !== null) {
      clearInterval(timer);
      timer = null;
    }
    button.textContent = paused ? "Play" : "Pause";
    button.setAttribute("aria-label", paused ? "Play the live house" : "Pause the live house");
  }
  const watcher = new IntersectionObserver((entries) => {
    for (const entry of entries) {
      if (entry.isIntersecting) shown.add(entry.target);
      else shown.delete(entry.target);
    }
    sync();
  });
  watcher.observe($("house").closest(".hero"));
  watcher.observe($("sensor-grid"));
  document.addEventListener("visibilitychange", sync);
  button.addEventListener("click", () => {
    paused = !paused;
    sync();
  });
  reducedMotion.addEventListener("change", () => {
    if (!reducedMotion.matches) return;
    renderer.flatten();
    paused = true;
    sync();
  });
  sync();

  return {
    watch,
    warmup,
    state: () => ({ completed: watch.completed(), running: timer !== null, paused }),
  };
}

const firstHour = (bits) => [...Array(24).keys()].find((h) => bits & (1 << h));
const lastHour = (bits) => [...Array(24).keys()].reverse().find((h) => bits & (1 << h)) + 1;

/** One learner pair's weights on `sensor` over the chapter 2 window, in kW per standard deviation. */
function trace(home, sensor, lmsSpeed, window) {
  const day = wasm.FamilyDay.withLmsStepSize(TRACE_SEED, lmsSpeed);
  day.run(window.start);
  const lms = [];
  const idbd = [];
  for (let t = 0; t < window.length; t += 1) {
    day.step();
    lms.push(day.lmsWeights()[sensor]);
    idbd.push(day.idbdWeights()[sensor]);
  }
  const stepSize = day.lmsStepSize();
  day.free();
  return { lms, idbd, stepSize };
}

/** The first minute, counted from `from`, at which `values` reach half of `goal`, or null. */
function halfway(values, goal, from) {
  const index = values.findIndex((v, i) => i >= from && v >= goal / 2);
  return index < 0 ? null : index - from;
}

const spread = (values) => {
  const mean = values.reduce((a, b) => a + b, 0) / values.length;
  return Math.sqrt(values.reduce((a, b) => a + (b - mean) ** 2, 0) / values.length);
};

function drawTrace(chart, run, truth, range, marks) {
  const width = 560;
  const height = 220;
  const n = truth.length;
  const x = (i) => ((i * width) / (n - 1)).toFixed(1);
  const y = (v) => (height - ((v - range[0]) / (range[1] - range[0])) * height).toFixed(1);
  const line = (values) => values.map((v, i) => `${x(i)},${y(v)}`).join(" ");
  chart.append(
    svg("line", { x1: 0, x2: width, y1: y(0), y2: y(0), class: "zero" }),
    ...marks.map((m) => svg("line", { x1: x(m - 0.5), x2: x(m - 0.5), y1: 0, y2: height, class: "marker" })),
    svg("polyline", { points: line(truth), class: "line-actual" }),
    svg("polyline", { points: line(run.lms), class: "line-lms" }),
    svg("polyline", { points: line(run.idbd), class: "line-idbd" }),
  );
}

function fillSpeeds(home) {
  const perDay = home.hours * home.readingsPerHour;
  const hours = home.sensors[LATE_SENSOR].hours;
  const on = TRACE_DAY * perDay + firstHour(hours) * home.readingsPerHour;
  const off = TRACE_DAY * perDay + lastHour(hours) * home.readingsPerHour;
  const window = { start: on - TRACE_MARGIN, length: off - on + 2 * TRACE_MARGIN };
  const inside = (i) => i >= TRACE_MARGIN && i < TRACE_MARGIN + off - on;
  const influence = home.sensors[LATE_SENSOR].influence;
  const lateTruth = Array.from({ length: window.length }, (_, i) => (inside(i) ? influence : 0));
  const late = trace(home, LATE_SENSOR, home.lmsStepSize, window);
  const nervous = trace(home, NERVOUS_SENSOR, NERVOUS_SPEED, window);
  const quietTruth = Array(window.length).fill(0);
  const marks = [TRACE_MARGIN, TRACE_MARGIN + off - on];

  drawTrace($("speed-low"), late, lateTruth, [-0.3, influence + 0.3], marks);
  const swing = Math.max(...nervous.lms.map(Math.abs), ...nervous.idbd.map(Math.abs)) * 1.1;
  drawTrace($("speed-high"), nervous, quietTruth, [-swing, swing], marks);

  const lmsHalf = halfway(late.lms, influence, TRACE_MARGIN);
  const idbdHalf = halfway(late.idbd, influence, TRACE_MARGIN);
  const peak = Math.max(...late.lms.slice(TRACE_MARGIN, TRACE_MARGIN + off - on));
  const minutes = off - on;
  const result = {
    window,
    late: { stepSize: late.stepSize, lmsHalf, idbdHalf, peak },
    nervous: { stepSize: nervous.stepSize, lmsSpread: spread(nervous.lms), idbdSpread: spread(nervous.idbd) },
  };
  $("speed-low-value").textContent = String(late.stepSize);
  $("speed-high-value").textContent = String(nervous.stepSize);
  $("speed-low").setAttribute(
    "aria-label",
    `Measured over ${minutes} minutes in which a sensor with small readings adds ${influence} kW per typical swing of its reading. ` +
      `At fixed speed ${late.stepSize} the trust in it peaks at ${peak.toFixed(2)}` +
      (lmsHalf === null ? " and never reaches half the real effect." : `, reaching half the real effect after ${lmsHalf} minutes.`) +
      (idbdHalf === null ? " IDBD does not reach half either." : ` IDBD reaches half after ${idbdHalf} minutes.`),
  );
  $("speed-high").setAttribute(
    "aria-label",
    `Measured over the same ${window.length} minutes on a sensor with large readings that never matters. At fixed speed ` +
      `${nervous.stepSize} the trust in it swings with a spread of ${result.nervous.lmsSpread.toFixed(2)}. IDBD's spread is ` +
      `${result.nervous.idbdSpread.toFixed(2)}.`,
  );
  $("late-caption").textContent =
    lmsHalf === null
      ? `At the tuned fixed speed the trust is still far below the real effect when the sensor stops mattering ${minutes} minutes later: it peaks at ${peak.toFixed(2)} of ${influence}.` +
        (idbdHalf === null ? "" : ` IDBD gets halfway in ${idbdHalf} minutes.`)
      : `At the tuned fixed speed the trust takes ${lmsHalf} minutes to get halfway.` + (idbdHalf === null ? "" : ` IDBD takes ${idbdHalf}.`);
  $("nervous-caption").textContent =
    `This sensor never matters, so the right trust is 0. At fixed speed ${nervous.stepSize} every reading knocks the trust about, ` +
    `with a spread of ${result.nervous.lmsSpread.toFixed(2)}. IDBD's spread is ${result.nervous.idbdSpread.toFixed(2)}.`;
  for (const note of document.querySelectorAll(".axis-note")) {
    note.style.gridTemplateColumns = `${TRACE_MARGIN}fr ${window.length - TRACE_MARGIN}fr`;
  }
  $("trace-seed").textContent = String(TRACE_SEED);
  return result;
}

/**
 * Explains the learned speeds against fair rivals: each sensor that matters is compared
 * with the sensors that never matter and started at its speed. Its own speed can fall, as
 * the oven's does, and still end well above theirs.
 */
function learnedWhy(home, start, speeds) {
  const ratios = [];
  let rose = 0;
  home.sensors.forEach((sensor, i) => {
    if (!sensor.hours) return;
    const rivals = home.sensors.flatMap((other, j) => (!other.hours && start[j] === start[i] ? [speeds[j]] : []));
    if (!rivals.length) return;
    if (speeds[i] > start[i]) rose += 1;
    ratios.push(speeds[i] / (rivals.reduce((a, b) => a + b, 0) / rivals.length));
  });
  const fell = ratios.length - rose;
  const words = ["None", "One", "Two", "Three", "Four"];
  const moved = `${words[rose] ?? rose} of the ${(words[ratios.length] ?? ratios.length).toLowerCase()} sped up and ${(words[fell] ?? fell).toLowerCase()} slowed down`;
  return (
    "Nobody told it which sensors matter. " +
    `${moved}, but each ended ${Math.min(...ratios).toFixed(1)} to ${Math.max(...ratios).toFixed(1)} times faster ` +
    "than the sensors that started at its speed and never matter. Their corrections kept agreeing, so their speeds " +
    "fell less or rose. The others' kept cancelling out."
  );
}

function fillDesk(home) {
  const day = new wasm.FamilyDay(home.seeds[0]);
  const start = Array.from(day.idbdSpeeds());
  day.run(home.readings);
  const speeds = Array.from(day.idbdSpeeds());
  day.free();
  const scale = Math.max(...speeds) * 1.1;
  speeds.forEach((speed, i) => {
    const height = `${((speed / scale) * 100).toFixed(1)}%`;
    const [track, fill, knob] = ["track", "fill", "knob"].map((name) => el("div", { class: name }));
    fill.style.height = height;
    knob.style.bottom = height;
    const fader = el("div", { class: "fader", "aria-hidden": "true" }, [track, fill, knob]);
    const label = el("span", { class: "fader-label" }, [
      home.sensors[i].name,
      el("br"),
      el("span", { class: "mono", text: speed.toFixed(3) }),
    ]);
    $("desk").append(el("li", { class: home.sensors[i].hours ? "relevant" : null }, [fader, label]));
  });
  $("desk-readings").textContent = count(home.readings);
  $("learned-why").textContent = learnedWhy(home, start, speeds);
  return speeds;
}

async function main() {
  await init();
  const home = JSON.parse(wasm.familyJson());
  $("core-version").textContent = `idbd ${wasm.coreVersion()}`;
  const relevant = home.sensors.filter((s) => s.hours).length;
  $("chip-sensors").textContent = `${home.sensors.length} sensors`;
  $("chip-relevant").textContent = `${relevant} matter`;
  fillHouseKey(home);
  const renderer = await mountRenderer(home);
  const hero = startHero(home, renderer);
  $("hero-warmup").textContent = `${home.heroWarmupDays} days, ${count(hero.warmup)} readings,`;
  const speeds = fillSpeeds(home);
  const desk = fillDesk(home);
  const slow = createSlowMotion(wasm, home, {
    next: $("slow-next"),
    run: $("slow-run"),
    runCount: $("slow-run-count"),
    restart: $("slow-restart"),
    reading: $("slow-reading"),
    guess: $("slow-guess"),
    actual: $("slow-actual"),
    miss: $("slow-miss"),
    summary: $("slow-summary"),
    rows: $("slow-rows"),
    hidden: $("slow-hidden"),
  });
  const family = createFamily(wasm, {
    root: $("family"),
    lanes: $("family-lanes"),
    run: $("family-run"),
    reset: $("family-reset"),
    clock: $("family-clock"),
    hint: $("family-hint"),
    key: $("family-key"),
    view: $("family-view"),
    viewButtons: [...document.querySelectorAll("#family-view [data-view]")],
    result: $("family-result"),
    resultTitle: $("family-result-title"),
    stage: $("family-stage"),
    dial: $("family-dial"),
    scores: $("family-scores"),
    mechanism: $("family-mechanism"),
    again: $("family-again"),
    failure: $("family-failure"),
    onReveal: () => slow.revealNames(),
  });

  window.idbdStory = {
    home,
    get renderer() {
      return renderer.kind;
    },
    hero: () => ({ ...hero.state(), warmup: hero.warmup, speeds: Array.from(hero.watch.speeds()) }),
    speeds,
    desk,
    family,
    game: () => family.state(),
    slow: () => slow.state(),
  };
  document.body.dataset.appReady = "true";
}

main().catch((error) => {
  document.body.dataset.appReady = "error";
  const note = document.createElement("p");
  note.className = "failure wrap";
  note.setAttribute("role", "alert");
  note.textContent = `The page could not start the idbd module: ${error.message}`;
  document.body.prepend(note);
});
