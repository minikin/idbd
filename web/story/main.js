// Boots the WebAssembly module and fills the story page. Every learning value on the
// page comes from the crate's HouseRace: the hero's glows and live chart, chapter 1's
// readings, chapter 2's fixed-speed traces, the slow motion, chapter 3's faders,
// chapter 4's lanes and the game.

import init, * as wasm from "../pkg/idbd_wasm.js";
import { el, svg } from "../src/dom.js";
import { fmtSigned } from "../src/format.js";
import { SENSOR_COLORS, createHouse2d } from "./house.js";
import { createFamily } from "./family.js";
import { createSlowMotion } from "./slow.js";

const HERO_SEED = 1;
const HERO_WARMUP = 5000;
// Fast enough that the relevant sensors switch every few seconds on screen.
const HERO_TICK_MS = 150;
const DESK_SEED = 1;
const DESK_READINGS = 30000;
const LANES_SEED = 2;
const LANES_WARMUP = 5000;
const LANES_SHOWN = 600;
const SPARK_POINTS = 60;
const GRID_POINTS = 25;

// Chapter 2 plots one real oven switch: the first one on this seed, after the learners
// have settled, with the oven off for BEFORE readings and then on for AFTER readings.
// 0.2 is about the fastest fixed speed that stays finite on this house: 0.3 diverges.
const SWITCH_SEED = 4;
const SWITCH_SEARCH_FROM = 1000;
const SWITCH_SEARCH_TO = 6000;
const SWITCH_BEFORE = 40;
const SWITCH_AFTER = 160;
const OVEN = 1;
const SLOW_SPEED = 0.005;
const FAST_SPEED = 0.2;

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

function fillHouseKey(house) {
  const key = $("house-key");
  house.sensors.forEach((name, i) => {
    const dot = el("i", { class: "sensor-dot", "aria-hidden": "true" });
    dot.style.background = SENSOR_COLORS[i];
    key.append(el("li", {}, [dot, name]));
  });
}

/**
 * Draws the house in 3D when WebGL works and motion is welcome, else in 2D. `flatten`
 * switches a running 3D house to 2D, for a lost WebGL context or a reduced-motion
 * preference turned on later.
 */
async function mountRenderer(house) {
  const node = $("house");
  const update2d = createHouse2d($("house-2d-sensors"), house.sensors);
  const renderer = { kind: "2d", update: update2d, flatten() {} };
  node.dataset.renderer = "2d";
  if (reducedMotion.matches) {
    return renderer;
  }
  try {
    const { mountHouse3d } = await import("./house3d.js");
    let latest = null;
    const view = mountHouse3d($("house-3d"), house.sensors, () => renderer.flatten());
    const canvas = $("house-3d");
    canvas.hidden = false;
    canvas.setAttribute("role", "img");
    canvas.setAttribute(
      "aria-label",
      "A cutaway house with six sensors. An amber ring around each sensor grows with the speed IDBD has learned for it.",
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

function startHero(house, renderer) {
  const race = new wasm.HouseRace(HERO_SEED);
  race.run(HERO_WARMUP);
  const readings = house.sensors.map(() => []);
  const actual = [];
  const guess = [];

  const sensors = house.sensors.map((name, i) => {
    const dot = el("i", { class: "sensor-dot" });
    dot.style.background = SENSOR_COLORS[i];
    const reading = el("span", { class: "sr-only" });
    const head = el("div", { class: "name" }, [
      el("span", {}, [dot, name, reading]),
      el("span", { class: "unknown", "aria-hidden": "true", text: "?" }),
    ]);
    const line = svg("polyline", { points: "" });
    const chart = svg("svg", { viewBox: "0 0 240 40", "aria-hidden": "true" }, [line]);
    $("sensor-grid").append(el("li", {}, [head, chart]));
    return { line, reading };
  });

  function record() {
    race.step();
    const x = race.features();
    readings.forEach((list, i) => {
      list.push(x[i]);
      if (list.length > GRID_POINTS) list.shift();
    });
    actual.push(race.target());
    guess.push(race.idbdPrediction());
    if (actual.length > SPARK_POINTS) {
      actual.shift();
      guess.shift();
    }
  }

  function draw() {
    const features = Array.from(race.features());
    renderer.update({ alphas: Array.from(race.idbdStepSizes()), features, strong: Array.from(race.strong()) });
    const minute = count(race.completed());
    $("live-count").textContent = `minute ${minute}`;
    $("live-actual").setAttribute("points", points(actual, 240, 56, 4));
    $("live-idbd").setAttribute("points", points(guess, 240, 56, 4));
    $("live-text").textContent =
      `Minute ${minute}: the room changed by ${fmtSigned(actual.at(-1), 2)}, ` +
      `IDBD guessed ${fmtSigned(guess.at(-1), 2)}.`;
    sensors.forEach(({ line, reading }, i) => {
      line.setAttribute("points", points(readings[i], 240, 40, 3));
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
    race,
    state: () => ({ completed: race.completed(), running: timer !== null, paused }),
  };
}

/** The first oven switch on SWITCH_SEED that has the off and on stretches chapter 2 needs. */
function findOvenSwitch() {
  const race = new wasm.HouseRace(SWITCH_SEED);
  const on = [];
  for (let t = 0; t < SWITCH_SEARCH_TO; t += 1) {
    race.step();
    on.push(race.strong()[OVEN] === 1);
  }
  race.free();
  for (let t = SWITCH_SEARCH_FROM; t + SWITCH_AFTER <= on.length; t += 1) {
    const before = on.slice(t - SWITCH_BEFORE, t);
    const after = on.slice(t, t + SWITCH_AFTER);
    if (before.every((x) => !x) && after.every((x) => x)) return t - SWITCH_BEFORE;
  }
  throw new Error(`seed ${SWITCH_SEED} has no long enough oven switch`);
}

function traceOven(start, speed) {
  const race = wasm.HouseRace.withLmsStepSize(SWITCH_SEED, speed);
  race.run(start);
  const trust = [];
  const truth = [];
  for (let t = 0; t < SWITCH_BEFORE + SWITCH_AFTER; t += 1) {
    race.step();
    trust.push(race.lmsWeights()[OVEN]);
    truth.push(race.coefficients()[OVEN]);
  }
  const stepSize = race.lmsStepSize();
  race.free();
  return { trust, truth, stepSize };
}

function fillSpeeds() {
  const start = findOvenSwitch();
  const runs = { low: traceOven(start, SLOW_SPEED), high: traceOven(start, FAST_SPEED) };
  const all = [...runs.low.trust, ...runs.high.trust, ...runs.low.truth];
  const lo = Math.min(...all) - 0.1;
  const hi = Math.max(...all) + 0.1;
  const width = 560;
  const height = 220;
  const n = SWITCH_BEFORE + SWITCH_AFTER;
  const x = (i) => ((i * width) / (n - 1)).toFixed(1);
  const y = (v) => (height - ((v - lo) / (hi - lo)) * height).toFixed(1);
  const line = (values) => values.map((v, i) => `${x(i)},${y(v)}`).join(" ");
  const marker = x(SWITCH_BEFORE - 0.5);

  for (const [name, run] of Object.entries(runs)) {
    const chart = $(`speed-${name}`);
    chart.append(
      svg("line", { x1: 0, x2: width, y1: y(0), y2: y(0), class: "zero" }),
      svg("line", { x1: marker, x2: marker, y1: 0, y2: height, class: "marker" }),
      svg("polyline", { points: line(run.truth), class: "line-actual" }),
      svg("polyline", { points: line(run.trust), class: name === "high" ? "line-lms thin" : "line-lms" }),
    );
    const after = run.trust.slice(SWITCH_BEFORE);
    const from = run.trust[SWITCH_BEFORE - 1].toFixed(2);
    const to = run.trust.at(-1).toFixed(2);
    const [off, onValue] = [run.truth[0], run.truth.at(-1)].map((v) => v.toFixed(1));
    chart.setAttribute(
      "aria-label",
      `Measured: the oven's real effect jumps from ${off} to ${onValue}. At fixed speed ${run.stepSize}, ` +
        `the trust in the oven goes from ${from} to ${to} over the next ${SWITCH_AFTER} readings, ` +
        `ranging from ${Math.min(...after).toFixed(2)} to ${Math.max(...after).toFixed(2)}.`,
    );
    $(`speed-${name}-value`).textContent = String(run.stepSize);
  }
  for (const note of document.querySelectorAll(".axis-note")) {
    note.style.gridTemplateColumns = `${SWITCH_BEFORE}fr ${SWITCH_AFTER}fr`;
  }
  $("switch-seed").textContent = String(SWITCH_SEED);
  $("switch-reading").textContent = count(start + SWITCH_BEFORE + 1);
  return { start, low: runs.low.trust, high: runs.high.trust, truth: runs.low.truth };
}

function fillDesk(house) {
  const race = new wasm.HouseRace(DESK_SEED);
  race.run(DESK_READINGS);
  const alphas = Array.from(race.idbdStepSizes());
  race.free();
  const scale = Math.max(...alphas) * 1.1;
  alphas.forEach((alpha, i) => {
    const height = `${((alpha / scale) * 100).toFixed(1)}%`;
    const [track, fill, knob] = ["track", "fill", "knob"].map((name) => el("div", { class: name }));
    fill.style.height = height;
    knob.style.bottom = height;
    const fader = el("div", { class: "fader", "aria-hidden": "true" }, [track, fill, knob]);
    const label = el("span", { class: "fader-label" }, [
      house.sensors[i],
      el("br"),
      el("span", { class: "mono", text: alpha.toFixed(3) }),
    ]);
    $("desk").append(el("li", { class: i < house.relevant ? "relevant" : null }, [fader, label]));
  });
  $("desk-readings").textContent = count(DESK_READINGS);
  return alphas;
}

function fillLanes(house) {
  const race = new wasm.HouseRace(LANES_SEED);
  race.run(LANES_WARMUP);
  const strong = Array.from({ length: house.relevant }, () => []);
  let idbd = 0;
  let lms = 0;
  for (let t = 0; t < LANES_SHOWN; t += 1) {
    race.step();
    race.strong().forEach((flag, k) => strong[k].push(flag === 1));
    idbd += (race.target() - race.idbdPrediction()) ** 2;
    lms += (race.target() - race.lmsPrediction()) ** 2;
  }
  const alphas = Array.from(race.idbdStepSizes());
  race.free();

  const lanes = $("lanes");
  house.sensors.forEach((sensorName, sensor) => {
    const flags = strong[sensor] ?? [];
    const on = flags.filter(Boolean).length;
    const summary = sensor < house.relevant ? `, strong for ${on} of ${LANES_SHOWN} readings` : ", never affects the room";
    const name = el("span", { class: sensor >= house.relevant ? "lane-name inert" : "lane-name" }, [
      sensorName,
      el("span", { class: "sr-only", text: summary }),
    ]);
    const strip = svg("svg", { viewBox: "0 0 1000 20", preserveAspectRatio: "none", "aria-hidden": "true" });
    strip.append(svg("rect", { x: 0, y: 0, width: 1000, height: 20, rx: 3, class: "lane-ground" }));
    let start = null;
    flags.forEach((isOn, i) => {
      if (isOn && start === null) start = i;
      if ((!isOn || i === flags.length - 1) && start !== null) {
        const end = isOn ? i + 1 : i;
        strip.append(
          svg("rect", {
            x: ((start * 1000) / LANES_SHOWN).toFixed(1),
            y: 0,
            width: (((end - start) * 1000) / LANES_SHOWN).toFixed(1),
            height: 20,
            class: "lane-on",
          }),
        );
        start = null;
      }
    });
    lanes.append(name, strip, el("span", { class: "speed", text: alphas[sensor].toFixed(3) }));
  });
  lanes.append(
    el("span", { class: "lane-spacer" }),
    el("span", { class: "mono faint", text: `reading 1 to ${LANES_SHOWN} →` }),
    el("span", { class: "mono faint speed", text: "speed" }),
  );
  $("lanes-shown").textContent = count(LANES_SHOWN);
  $("lanes-speed-at").textContent = count(LANES_WARMUP + LANES_SHOWN);

  // The same fixed scale as the game. Both ends are long-run averages for the switching
  // house: 100% is the measurement noise alone, 0% is always guessing "no change".
  const noise = house.noise ** 2;
  const nothingLongRun = house.levels.reduce((sum, [hi, lo]) => sum + (hi ** 2 + lo ** 2) / 2, 0) + noise;
  const accuracyOf = (mse) => Math.max(0, Math.min(1, (nothingLongRun - mse) / (nothingLongRun - noise)));
  const result = { idbd: idbd / LANES_SHOWN, lms: lms / LANES_SHOWN, strong };
  result.idbdAccuracy = accuracyOf(result.idbd);
  result.lmsAccuracy = accuracyOf(result.lms);
  $("run-idbd").textContent = `${Math.round(result.idbdAccuracy * 100)}%`;
  $("run-lms").textContent = `${Math.round(result.lmsAccuracy * 100)}%`;
  return result;
}

async function main() {
  await init();
  const house = JSON.parse(wasm.houseJson());
  $("core-version").textContent = `idbd ${wasm.coreVersion()}`;
  $("chip-sensors").textContent = `${house.sensors.length} sensors`;
  $("chip-relevant").textContent = `${house.relevant} matter`;
  $("hero-warmup").textContent = count(HERO_WARMUP);
  fillHouseKey(house);
  const renderer = await mountRenderer(house);
  const hero = startHero(house, renderer);
  const speeds = fillSpeeds();
  const desk = fillDesk(house);
  const lanes = fillLanes(house);
  const slow = createSlowMotion(wasm, house, {
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
  });

  window.idbdStory = {
    house,
    get renderer() {
      return renderer.kind;
    },
    hero: () => ({ ...hero.state(), stepSizes: Array.from(hero.race.idbdStepSizes()) }),
    speeds,
    desk,
    lanes,
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
