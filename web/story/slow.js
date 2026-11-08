// Slow motion: one reading at a time from a fresh FamilyDay, with every change IDBD makes
// read from the crate before and after the update. Readings, trust and speeds are all per
// typical swing of each sensor's reading (its standard deviation), so sensors that read on
// different scales compare directly and reading times trust adds up to the guess.
//
// The rows are lettered and shuffled until the visitor has played the game, so this
// chapter does not give away which sensors matter.

import { svg } from "../src/dom.js";
import { fmtSigned } from "../src/format.js";

const SEED = 3;
const HISTORY = 600;
/** The sensor shown in each row, a fixed shuffle so row letters do not follow chapter 1. */
const ORDER = [6, 1, 10, 3, 8, 0, 11, 5, 2, 9, 4, 7];

const fmt = (v, digits = 2) => fmtSigned(v, digits);

/** A power draw against the base in words: "0.74 kW above the base" or "at the base". */
export function inWords(v) {
  const size = Math.abs(v).toFixed(2);
  if (size === "0.00") return "at the base";
  return `${size} kW ${v > 0 ? "above" : "below"} the base`;
}

/** How far the guess missed, and which way: "0.75 kW too low". */
export function missInWords(target, guess) {
  const miss = target - guess;
  const size = Math.abs(miss).toFixed(2);
  if (size === "0.00") return "spot on";
  return `${size} kW ${miss > 0 ? "too low" : "too high"}`;
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

export function createSlowMotion(wasm, home, nodes) {
  let race = null;
  let history = [];
  let last = null;
  const RUN = home.readings;

  nodes.runCount.textContent = RUN.toLocaleString("en-US");
  nodes.run.textContent = "Run three days";

  const letters = new Map(ORDER.map((sensor, row) => [sensor, `Sensor ${String.fromCharCode(65 + row)}`]));
  let named = false;
  const label = (s) => (named ? home.sensors[s].name : letters.get(s));

  const rows = ORDER.map((s) => {
    const row = document.createElement("tr");
    const head = document.createElement("th");
    head.scope = "row";
    head.textContent = label(s);
    const cells = ["reading", "trust", "speed"].map(() => document.createElement("td"));
    const sparkCell = document.createElement("td");
    const line = svg("polyline");
    const chart = svg("svg", { viewBox: "0 0 120 24", "aria-hidden": "true" }, [line]);
    sparkCell.append(chart);
    row.append(head, ...cells, sparkCell);
    nodes.rows.append(row);
    return { s, head, reading: cells[0], trust: cells[1], speed: cells[2], line };
  });

  function restart() {
    if (race) race.free();
    race = new wasm.FamilyDay(SEED);
    history = Array.from(race.idbdSpeeds(), (a) => [a]);
    last = null;
    render();
  }

  function advance(wanted) {
    const count = Math.min(wanted, RUN - race.completed());
    if (count < 1) return;
    for (let i = 0; i < count - 1; i += 1) {
      race.step();
      race.idbdSpeeds().forEach((a, s) => history[s].push(a));
    }
    const before = { alphas: Array.from(race.idbdSpeeds()), weights: Array.from(race.idbdWeights()) };
    race.step();
    const alphas = Array.from(race.idbdSpeeds());
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
    const over = race.completed() >= RUN;
    nodes.next.disabled = over;
    nodes.run.disabled = over;
    nodes.reading.textContent = race.completed().toLocaleString("en-US");
    const top = Math.max(...history.flat()) * 1.05;
    for (const row of rows) row.line.setAttribute("points", sparkline(history[row.s], top));
    if (!last) {
      for (const node of [nodes.guess, nodes.actual, nodes.miss]) node.textContent = "-";
      const start = home.initialStepSize.toFixed(4);
      const ratio = Math.max(...history.map((h) => h[0])) / Math.min(...history.map((h) => h[0]));
      nodes.summary.textContent =
        `Press Next reading to start. Every sensor begins with trust 0 and the same step size, ${start}. ` +
        `Per typical swing of a reading, that is already ${ratio.toFixed(0)} times faster on the sensors whose readings swing most.`;
      for (const row of rows) {
        row.reading.textContent = "-";
        row.trust.textContent = fmt(0);
        row.speed.textContent = history[row.s][0].toFixed(4);
        row.speed.className = "";
      }
      return;
    }
    nodes.guess.textContent = inWords(last.guess);
    nodes.actual.textContent = inWords(last.target);
    nodes.miss.textContent = missInWords(last.target, last.guess);

    let biggest = 0;
    let up = 0;
    let down = 0;
    rows.forEach((row) => {
      const { s } = row;
      const w0 = last.before.weights[s];
      const w1 = last.weights[s];
      const a0 = last.before.alphas[s];
      const a1 = last.alphas[s];
      if (Math.abs(w1 - w0) > Math.abs(last.weights[biggest] - last.before.weights[biggest])) biggest = s;
      if (a1 > a0) up += 1;
      if (a1 < a0) down += 1;
      row.reading.textContent = fmt(last.features[s] / home.sensors[s].scale);
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
    const end = over ? " The three days are over. Restart to watch again." : "";
    nodes.summary.textContent =
      `IDBD guessed the meter would read ${inWords(last.guess)}. It read ${inWords(last.target)}, so the guess was ` +
      `${missInWords(last.target, last.guess)}. It moved its trust in ${named ? "the " : ""}${label(biggest)} the most, ` +
      `by ${fmt(change, 3)}. ${speeds}${end}`;
  }

  nodes.next.addEventListener("click", () => advance(1));
  nodes.run.addEventListener("click", () => advance(RUN));
  nodes.restart.addEventListener("click", restart);
  restart();

  return {
    /** Puts the sensors' names on the rows, once the game has revealed which matter. */
    revealNames() {
      named = true;
      for (const row of rows) row.head.textContent = label(row.s);
      nodes.hidden.hidden = true;
      if (last) render();
    },
    state() {
      return {
        order: ORDER,
        named,
        readings: last && last.features.map((v, s) => v / home.sensors[s].scale),
        trustBefore: last && last.before.weights,
        completed: race.completed(),
        stepSizes: Array.from(race.idbdSpeeds()),
        last: last && { guess: last.guess, target: last.target, before: last.before.alphas, after: last.alphas },
      };
    },
  };
}
