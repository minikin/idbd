// Boots the WebAssembly module and fills the story page. Every learning value on the
// page comes from the crate's HouseRace: the hero's glows and live chart, chapter 1's
// readings, chapter 3's faders, chapter 4's lanes and the game.

import init, * as wasm from "../pkg/idbd_wasm.js";
import { SENSOR_COLORS, createHouse2d } from "./house.js";
import { createGame } from "./game.js";
import { createSlowMotion } from "./slow.js";

const HERO_SEED = 1;
const HERO_WARMUP = 5000;
const DESK_SEED = 1;
const DESK_READINGS = 30000;
const LANES_SEED = 2;
const LANES_WARMUP = 5000;
const LANES_SHOWN = 600;
const SPARK_POINTS = 60;
const GRID_POINTS = 25;

const $ = (id) => document.getElementById(id);

function points(values, width, height, range) {
  const step = width / Math.max(1, values.length - 1);
  return values
    .map((v, i) => {
      const y = height / 2 - (Math.max(-range, Math.min(range, v)) / range) * (height / 2 - 2);
      return `${(i * step).toFixed(1)},${y.toFixed(1)}`;
    })
    .join(" ");
}

function svg(name, attributes) {
  const node = document.createElementNS("http://www.w3.org/2000/svg", name);
  for (const [key, value] of Object.entries(attributes)) {
    node.setAttribute(key, String(value));
  }
  return node;
}

async function mountRenderer(house) {
  const update2d = createHouse2d($("house-2d-sensors"), house.sensors);
  const reduced = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
  if (reduced) {
    return { kind: "2d", update: update2d };
  }
  try {
    const { mountHouse3d } = await import("./house3d.js");
    const view = mountHouse3d($("house-3d"), house.sensors);
    $("house-3d").hidden = false;
    $("house-3d").setAttribute("role", "img");
    $("house-3d").setAttribute(
      "aria-label",
      "A cutaway house with twelve sensors. Each sensor glows as wide as the speed IDBD has learned for it.",
    );
    $("house-2d").style.visibility = "hidden";
    $("house-2d").setAttribute("aria-hidden", "true");
    return {
      kind: "3d",
      update(state) {
        update2d(state);
        view.update(state);
      },
    };
  } catch (error) {
    console.warn("The 3D house is unavailable, so the page draws it in 2D.", error);
    return { kind: "2d", update: update2d };
  }
}

function startHero(house, renderer) {
  const race = new wasm.HouseRace(HERO_SEED);
  race.run(HERO_WARMUP);
  const readings = house.sensors.map(() => []);
  const actual = [];
  const guess = [];
  let visible = true;

  const grid = $("sensor-grid");
  const sparks = house.sensors.map((name) => {
    const item = document.createElement("li");
    const head = document.createElement("div");
    head.className = "name";
    const label = document.createElement("span");
    const dot = document.createElement("i");
    dot.className = "sensor-dot";
    dot.style.background = SENSOR_COLORS[house.sensors.indexOf(name)];
    label.append(dot, name);
    const unknown = document.createElement("span");
    unknown.className = "unknown";
    unknown.setAttribute("aria-hidden", "true");
    unknown.textContent = "?";
    head.append(label, unknown);
    const chart = svg("svg", { viewBox: "0 0 240 40", "aria-hidden": "true" });
    const line = svg("polyline", { points: "" });
    chart.append(line);
    item.append(head, chart);
    grid.append(item);
    return line;
  });

  function draw() {
    renderer.update({
      alphas: Array.from(race.idbdStepSizes()),
      features: Array.from(race.features()),
      strong: Array.from(race.strong()),
    });
    $("live-count").textContent = `minute ${race.completed().toLocaleString("en-US")}`;
    $("live-actual").setAttribute("points", points(actual, 240, 56, 4));
    $("live-idbd").setAttribute("points", points(guess, 240, 56, 4));
    sparks.forEach((line, i) => line.setAttribute("points", points(readings[i], 240, 40, 3)));

  }

  function step() {
    if (!visible || document.hidden) {
      return;
    }
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
    draw();
  }

  for (let i = 0; i < SPARK_POINTS; i += 1) {
    race.step();
    const x = race.features();
    readings.forEach((list, s) => {
      list.push(x[s]);
      if (list.length > GRID_POINTS) list.shift();
    });
    actual.push(race.target());
    guess.push(race.idbdPrediction());
  }
  draw();

  new IntersectionObserver((entries) => {
    visible = entries.some((entry) => entry.isIntersecting);
  }).observe($("top"));
  // Fast enough that the relevant sensors switch every few seconds on screen.
  setInterval(step, 150);
  return race;
}

function fillDesk(house) {
  const race = new wasm.HouseRace(DESK_SEED);
  race.run(DESK_READINGS);
  const alphas = Array.from(race.idbdStepSizes());
  race.free();
  const scale = Math.max(...alphas) * 1.1;
  const desk = $("desk");
  alphas.forEach((alpha, i) => {
    const item = document.createElement("li");
    if (i < house.relevant) item.className = "relevant";
    const fader = document.createElement("div");
    fader.className = "fader";
    fader.setAttribute("aria-hidden", "true");
    const height = `${((alpha / scale) * 100).toFixed(1)}%`;
    const [track, fill, knob] = ["track", "fill", "knob"].map((name) => {
      const part = document.createElement("div");
      part.className = name;
      return part;
    });
    fill.style.height = height;
    knob.style.bottom = height;
    fader.append(track, fill, knob);
    const label = document.createElement("span");
    label.className = "fader-label";
    label.append(house.sensors[i]);
    label.append(document.createElement("br"));
    const value = document.createElement("span");
    value.className = "mono";
    value.textContent = alpha.toFixed(3);
    label.append(value);
    item.append(fader, label);
    desk.append(item);
  });
  $("desk-readings").textContent = DESK_READINGS.toLocaleString("en-US");
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
    const flags = race.strong();
    flags.forEach((flag, k) => strong[k].push(flag === 1));
    idbd += (race.target() - race.idbdPrediction()) ** 2;
    lms += (race.target() - race.lmsPrediction()) ** 2;
  }
  const alphas = Array.from(race.idbdStepSizes());
  race.free();

  const lanes = $("lanes");
  const rows = house.sensors.map((_, sensor) => ({ sensor, flags: strong[sensor] ?? [] }));
  for (const { sensor, flags } of rows) {
    const name = document.createElement("span");
    name.textContent = house.sensors[sensor];
    name.className = sensor >= house.relevant ? "lane-name inert" : "lane-name";
    const strip = svg("svg", { viewBox: "0 0 1000 20", preserveAspectRatio: "none", "aria-hidden": "true" });
    strip.append(svg("rect", { x: 0, y: 0, width: 1000, height: 20, rx: 3, class: "lane-ground" }));
    let start = null;
    flags.forEach((on, i) => {
      if (on && start === null) start = i;
      if ((!on || i === flags.length - 1) && start !== null) {
        const end = on ? i + 1 : i;
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
    const speed = document.createElement("span");
    speed.className = "speed";
    speed.textContent = alphas[sensor].toFixed(3);
    lanes.append(name, strip, speed);
  }
  const axis = document.createElement("span");
  const axisLabel = document.createElement("span");
  axisLabel.className = "mono faint";
  axisLabel.textContent = `reading 1 to ${LANES_SHOWN} →`;
  const speedLabel = document.createElement("span");
  speedLabel.className = "mono faint speed";
  speedLabel.textContent = "speed";
  lanes.append(axis, axisLabel, speedLabel);

  // The same fixed scale as the game: 0% is the switching house's long-run error for
  // always guessing "no change".
  const nothingLongRun = house.levels.reduce((sum, [strong, weak]) => sum + (strong ** 2 + weak ** 2) / 2, 0) + 0.25;
  const accuracyOf = (mse) => Math.max(0, Math.min(1, (nothingLongRun - mse) / (nothingLongRun - 0.25)));
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
  const renderer = await mountRenderer(house);
  const hero = startHero(house, renderer);
  const desk = fillDesk(house);
  const lanes = fillLanes(house);
  const slow = createSlowMotion(wasm, house, {
    next: $("slow-next"),
    run: $("slow-run"),
    restart: $("slow-restart"),
    reading: $("slow-reading"),
    guess: $("slow-guess"),
    actual: $("slow-actual"),
    miss: $("slow-miss"),
    summary: $("slow-summary"),
    rows: $("slow-rows"),
  });
  const game = createGame(wasm, house, {
    levels: [...document.querySelectorAll(".level")],
    levelNote: $("level-note"),
    chart: $("game-chart"),
    run: $("game-run"),
    reset: $("game-reset"),
    sliders: $("sliders"),
    scores: $("scores"),
    count: $("game-count"),
    you: $("game-you"),
    idbd: $("game-idbd"),
    lms: $("game-lms"),
    changes: $("game-changes"),
    reveal: $("reveal"),
    learned: $("learned"),
    learnedWrap: $("learned-wrap"),
    learnedNote: $("learned-note"),
    verdict: $("verdict"),
    failure: $("game-failure"),
  });

  window.idbdStory = {
    house,
    renderer: renderer.kind,
    hero: () => ({ completed: hero.completed(), stepSizes: Array.from(hero.idbdStepSizes()) }),
    desk,
    lanes,
    game: () => game.state(),
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
