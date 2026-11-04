// Slow motion: one reading at a time from a fresh HouseRace, with every change IDBD makes
// read from the crate before and after the update.

import { svg } from "../src/dom.js";
import { fmtSigned } from "../src/format.js";

const SEED = 3;
const RUN = 5000;
const HISTORY = 600;

const fmt = (v, digits = 2) => fmtSigned(v, digits);

/** A temperature change in words: "warmer by 0.74", "cooler by 0.16" or "no change". */
export function inWords(v) {
  const size = Math.abs(v).toFixed(2);
  if (size === "0.00") return "no change";
  return `${v > 0 ? "warmer" : "cooler"} by ${size}`;
}

/** How far the guess missed, and which way: "0.75 too low". */
export function missInWords(target, guess) {
  const miss = target - guess;
  const size = Math.abs(miss).toFixed(2);
  if (size === "0.00") return "spot on";
  return `${size} ${miss > 0 ? "too low" : "too high"}`;
}

/** A relative change that keeps two significant digits however small it is. */
function percent(a0, a1) {
  const value = ((a1 - a0) / a0) * 100;
  const digits = Math.max(1, 1 - Math.floor(Math.log10(Math.abs(value))));
  return `${value > 0 ? "+" : "-"}${Math.abs(value).toFixed(Math.min(digits, 6))}%`;
}

/** Every row shares one scale from zero, so a speed that grows reads taller than one that shrinks. */
function sparkline(values, top) {
  if (values.length < 2) {
    return "";
  }
  return values
    .map((v, i) => `${((i * 120) / (values.length - 1)).toFixed(1)},${(22 - (v / top) * 20).toFixed(1)}`)
    .join(" ");
}

export function createSlowMotion(wasm, house, nodes) {
  let race = null;
  let history = [];
  let last = null;

  nodes.runCount.textContent = RUN.toLocaleString("en-US");
  nodes.run.textContent = `Run ${RUN.toLocaleString("en-US")}`;

  const rows = house.sensors.map((name, sensor) => {
    const row = document.createElement("tr");
    if (sensor < house.relevant) row.className = "relevant";
    const head = document.createElement("th");
    head.scope = "row";
    head.textContent = name;
    const cells = ["reading", "trust", "speed"].map(() => document.createElement("td"));
    const sparkCell = document.createElement("td");
    const line = svg("polyline");
    const chart = svg("svg", { viewBox: "0 0 120 24", "aria-hidden": "true" }, [line]);
    sparkCell.append(chart);
    row.append(head, ...cells, sparkCell);
    nodes.rows.append(row);
    return { reading: cells[0], trust: cells[1], speed: cells[2], line };
  });

  function restart() {
    if (race) race.free();
    race = new wasm.HouseRace(SEED);
    history = Array.from(race.idbdStepSizes(), (a) => [a]);
    last = null;
    render();
  }

  function advance(count) {
    for (let i = 0; i < count - 1; i += 1) {
      race.step();
      race.idbdStepSizes().forEach((a, s) => history[s].push(a));
    }
    const before = { alphas: Array.from(race.idbdStepSizes()), weights: Array.from(race.idbdWeights()) };
    race.step();
    const alphas = Array.from(race.idbdStepSizes());
    alphas.forEach((a, s) => {
      history[s].push(a);
      history[s].splice(0, history[s].length - HISTORY);
    });
    last = {
      before,
      alphas,
      weights: Array.from(race.idbdWeights()),
      features: Array.from(race.features()),
      target: race.target(),
      guess: race.idbdPrediction(),
    };
    render();
  }

  function render() {
    nodes.reading.textContent = race.completed().toLocaleString("en-US");
    const top = Math.max(...history.flat()) * 1.05;
    rows.forEach((row, s) => row.line.setAttribute("points", sparkline(history[s], top)));
    if (!last) {
      for (const node of [nodes.guess, nodes.actual, nodes.miss]) node.textContent = "-";
      const start = house.initialStepSize.toFixed(4);
      nodes.summary.textContent = `Press Next reading to start. Every sensor begins with trust 0 and the same speed, ${start}.`;
      rows.forEach((row, s) => {
        row.reading.textContent = "-";
        row.trust.textContent = fmt(0);
        row.speed.textContent = history[s][0].toFixed(4);
        row.speed.className = "";
      });
      return;
    }
    nodes.guess.textContent = inWords(last.guess);
    nodes.actual.textContent = inWords(last.target);
    nodes.miss.textContent = missInWords(last.target, last.guess);

    let biggest = 0;
    let up = 0;
    let down = 0;
    rows.forEach((row, s) => {
      const w0 = last.before.weights[s];
      const w1 = last.weights[s];
      const a0 = last.before.alphas[s];
      const a1 = last.alphas[s];
      if (Math.abs(w1 - w0) > Math.abs(last.weights[biggest] - last.before.weights[biggest])) biggest = s;
      if (a1 > a0) up += 1;
      if (a1 < a0) down += 1;
      row.reading.textContent = fmt(last.features[s]);
      row.trust.textContent = `${fmt(w0)} → ${fmt(w1)}`;
      const move = a1 === a0 ? "unchanged" : `${a1 > a0 ? "↑" : "↓"} ${percent(a0, a1)}`;
      row.speed.textContent = `${a0.toFixed(4)} → ${a1.toFixed(4)} ${move}`;
      row.speed.className = a1 === a0 ? "flat" : a1 > a0 ? "up" : "down";
    });
    const change = last.weights[biggest] - last.before.weights[biggest];
    const speeds =
      race.completed() === 1
        ? "No speed moved: a speed changes only when a correction can be compared with earlier ones, and this is the first."
        : `${up} speeds went up and ${down} went down. A speed goes up when that sensor's correction points ` +
          "the same way as its recent ones.";
    nodes.summary.textContent =
      `IDBD guessed the room would get ${inWords(last.guess)}. It got ${inWords(last.target)}, so the guess was ` +
      `${missInWords(last.target, last.guess)}. It moved its trust in the ${house.sensors[biggest]} the most, ` +
      `by ${fmt(change, 3)}. ${speeds}`;
  }

  nodes.next.addEventListener("click", () => advance(1));
  nodes.run.addEventListener("click", () => advance(RUN));
  nodes.restart.addEventListener("click", restart);
  restart();

  return {
    state() {
      return {
        completed: race.completed(),
        stepSizes: Array.from(race.idbdStepSizes()),
        last: last && { guess: last.guess, target: last.target, before: last.before.alphas, after: last.alphas },
      };
    },
  };
}
