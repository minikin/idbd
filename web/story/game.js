import { SENSOR_COLORS } from "./house.js";

// "Play against it": the visitor's trust sliders against IDBD and a tuned fixed-step
// learner, both computed by the crate's HouseRace on the same readings. Level 1 is the
// still house, level 2 the switching house.

const SEED = 20261103;
const WARMUP = 5000;
const TICK_MS = 100;
/** One round is 600 readings, a minute at 10 readings a second. Scores cover the round. */
const ROUND = 600;
const SMOOTH = 40;
const CHART = ROUND;
/** Readings before a leader is named, so one lucky guess does not win. */
const MIN_SCORED = 50;
/** A slider within 0.15 of the truth counts as close: 0.15 squared. */
const CLOSE = 0.0225;
/** IDBD's starting step size, the reference for "faster" and "slower". */
const START_SPEED = 0.05;
/** The noise alone keeps even a perfect guess this far off, in degrees: sqrt(0.25). */
const NOISE_FLOOR = 0.5;

const LEVEL_NOTES = {
  still:
    "Nothing changes in this house. Set every slider right and you will be off by about 0.50 \u00b0C, the noise alone.",
  changing:
    "The sun, the oven and the front door switch between a strong and a weak effect about every two seconds. You will lose this one, and so will the fixed speed. Keeping up is what IDBD is for.",
};

const LEARNED_NOTES = {
  still:
    "Nothing changes in this house, so IDBD has calmed down on every sensor and their speeds are about equal. That is the right answer here, and it is why the fixed speed can win this level.",
  changing:
    "Here the sun, the oven and the front door keep switching, so IDBD keeps learning fast about them and slows down on the other three. A sensor can barely matter right now and still be watched closely, ready for its next change.",
};

/** The verdict at the end of a round, by level and winner. */
const VERDICTS = {
  still: {
    you: "You win. You found how much each sensor matters, and in a calm house that never changes. Now try Level 2, where it does.",
    idbd: "IDBD wins this round, narrowly. In a calm house the fixed speed usually edges ahead. Try Level 2 to see where IDBD pulls away.",
    lms: "The fixed speed wins. That is expected: in a calm house one well-chosen speed is enough, and IDBD pays a little for staying ready to change. Now try Level 2.",
  },
  changing: {
    you: "You beat both learners while the house kept changing. That is rare.",
    idbd: "IDBD wins. It kept up by learning fast only about the sun, the oven and the front door.",
    lms: "The fixed speed won this round. Over many rounds of this house IDBD comes out ahead: closer in all 10 of our simulated runs.",
  },
};

/** Plain words for a slider value. */
function meaning(value) {
  const size = Math.abs(value);
  if (size < 0.15) return "ignore it";
  const verb = value > 0 ? "warms" : "cools";
  return size >= 0.8 ? `${verb} the room a lot` : `${verb} the room a little`;
}

/** Typical miss in degrees: the square root of the average squared miss. */
const typical = (values) => (values.length ? Math.sqrt(values.reduce((a, b) => a + b, 0) / values.length) : null);


function keep(list, value, size) {
  list.push(value);
  if (list.length > size) list.shift();
}

/** Fills a slider's rail from its centre, 0, to the thumb. */
function paintRail(input) {
  const at = ((Number(input.value) - Number(input.min)) / (Number(input.max) - Number(input.min))) * 100;
  input.style.setProperty("--lo", `${Math.min(50, at)}%`);
  input.style.setProperty("--hi", `${Math.max(50, at)}%`);
}

export function createGame(wasm, house, nodes) {
  let level = "still";
  let race = null;
  let running = false;
  let timer = null;
  let trustValues = house.sensors.map(() => 0);
  let scored = 0;
  let squared = { you: [], idbd: [], lms: [] };
  let recent = { you: [], idbd: [], lms: [] };
  let series = { you: [], idbd: [], lms: [] };
  let changes = [];
  let failure = null;
  let finished = false;
  let hints = false;

  const sliders = house.sensors.map((name, sensor) => {
    const id = `trust-${sensor}`;
    const wrap = document.createElement("div");
    wrap.className = "slider";
    const row = document.createElement("div");
    row.className = "slider-row";
    const label = document.createElement("label");
    label.htmlFor = id;
    const dot = document.createElement("i");
    dot.className = "sensor-dot";
    dot.style.background = SENSOR_COLORS[sensor];
    label.append(dot, name);
    const input = document.createElement("input");
    Object.assign(input, { id, type: "range", min: "-1.5", max: "1.5", step: "0.1", value: "0" });
    const output = document.createElement("output");
    output.htmlFor = id;
    output.textContent = "0.0";
    const cost = document.createElement("p");
    cost.className = "cost";
    cost.id = `${id}-cost`;
    input.setAttribute("aria-describedby", cost.id);
    input.addEventListener("input", () => {
      trustValues[sensor] = Number(input.value);
      output.textContent = trustValues[sensor].toFixed(1);
      paintRail(input);
      renderCosts();
    });
    row.append(label, input, output);
    wrap.append(row, cost);
    nodes.sliders.append(wrap);
    return { input, output, cost };
  });

  const players = [
    { key: "you", name: "You", color: "var(--you)", caption: () => "guessing with your sliders" },
    { key: "idbd", name: "IDBD", color: "var(--idbd)", caption: () => "tunes its own speed for each sensor" },
    {
      key: "lms",
      name: "Fixed speed",
      color: "var(--lms)",
      caption: () => "one speed for every sensor, the best one for this house",
    },
  ].map((player) => {
    const root = document.createElement("div");
    root.className = "score";
    const who = document.createElement("span");
    who.className = "who";
    const swatch = document.createElement("i");
    swatch.style.background = player.color;
    who.append(swatch, player.name);
    const prefix = document.createElement("span");
    prefix.className = "prefix";
    prefix.textContent = "typically off by";
    const value = document.createElement("span");
    value.className = "value";
    const caption = document.createElement("span");
    caption.className = "note";
    const lead = document.createElement("span");
    lead.className = "lead";
    root.append(who, prefix, value, caption, lead);
    nodes.scores.append(root);
    return { ...player, root, value, captionNode: caption, lead };
  });

  const learned = house.sensors.map((name, sensor) => {
    const item = document.createElement("div");
    item.className = sensor < house.relevant ? "item relevant" : "item";
    const row = document.createElement("div");
    row.className = "row";
    const label = document.createElement("span");
    label.textContent = name;
    const weight = document.createElement("span");
    weight.className = "muted";
    row.append(label, weight);
    const bar = document.createElement("div");
    bar.className = "bar";
    const fill = document.createElement("i");
    bar.append(fill);
    const speed = document.createElement("span");
    speed.className = "mono faint";
    item.append(row, bar, speed);
    nodes.learned.append(item);
    return { weight, fill, speed };
  });

  const SVG = "http://www.w3.org/2000/svg";
  const floor = document.createElementNS(SVG, "line");
  floor.setAttribute("class", "floor-line");
  nodes.chart.insertBefore(floor, nodes.chart.firstChild);
  const floorLabel = document.createElementNS(SVG, "text");
  floorLabel.setAttribute("class", "chart-label");
  floorLabel.setAttribute("x", "596");
  floorLabel.setAttribute("text-anchor", "end");
  floorLabel.textContent = "noise alone";
  const youOff = document.createElementNS(SVG, "text");
  youOff.setAttribute("class", "chart-label you-label");
  youOff.setAttribute("x", "596");
  youOff.setAttribute("y", "14");
  youOff.setAttribute("text-anchor", "end");
  const ticks = [0.5, 1, 1.5, 2].map((value) => {
    const text = document.createElementNS(SVG, "text");
    text.setAttribute("class", "chart-label");
    text.setAttribute("x", "2");
    return { text, value };
  });
  nodes.chart.append(floorLabel, youOff, ...ticks.map((t) => t.text));

  function newRace() {
    if (race) race.free();
    race = level === "still" ? wasm.HouseRace.still(SEED) : new wasm.HouseRace(SEED);
    race.run(WARMUP);
  }

  function start() {
    if (finished) reset();
    if (running || failure) return;
    running = true;
    timer = setInterval(tick, TICK_MS);
    render();
  }

  function stop() {
    running = false;
    clearInterval(timer);
    timer = null;
    render();
  }

  function reset() {
    running = false;
    clearInterval(timer);
    timer = null;
    newRace();
    trustValues = house.sensors.map(() => 0);
    for (const slider of sliders) {
      slider.input.value = "0";
      slider.output.textContent = "0.0";
      paintRail(slider.input);
    }
    scored = 0;
    squared = { you: [], idbd: [], lms: [] };
    recent = { you: [], idbd: [], lms: [] };
    series = { you: [], idbd: [], lms: [] };
    changes = [];
    failure = null;
    finished = false;
    nodes.verdict.hidden = true;
    nodes.failure.hidden = true;
    nodes.levelNote.textContent = LEVEL_NOTES[level];
    nodes.learnedNote.textContent = LEARNED_NOTES[level];
    setRevealed(false);
    render();
  }

  function setLevel(next) {
    level = next;
    for (const button of nodes.levels) {
      button.setAttribute("aria-pressed", String(button.dataset.level === level));
    }
    reset();
  }

  function tick() {
    try {
      race.step();
    } catch (error) {
      failure = race.failureMessage() ?? String(error);
      nodes.failure.textContent = `The race stopped: ${failure}`;
      nodes.failure.hidden = false;
      stop();
      return;
    }
    const x = race.features();
    const y = race.target();
    let you = 0;
    for (let s = 0; s < x.length; s += 1) you += trustValues[s] * x[s];
    const misses = { you: (y - you) ** 2, idbd: (y - race.idbdPrediction()) ** 2, lms: (y - race.lmsPrediction()) ** 2 };
    for (const key of Object.keys(misses)) {
      squared[key].push(misses[key]);
      keep(recent[key], misses[key], SMOOTH);
      keep(series[key], typical(recent[key]), CHART);
    }
    keep(changes, race.switched().length > 0, CHART);
    scored += 1;
    if (scored >= ROUND) {
      finish();
      return;
    }
    render();
  }

  function finish() {
    finished = true;
    running = false;
    clearInterval(timer);
    timer = null;
    const errors = scores();
    const order = Object.keys(errors).sort((a, b) => errors[a] - errors[b]);
    const [first, second] = order;
    const names = { you: "You", idbd: "IDBD", lms: "The fixed speed" };
    const closer = Math.round((1 - errors[first] / errors[second]) * 100);
    const switches = changes.filter(Boolean).length;
    const changed = level === "changing" ? ` What matters changed ${switches} times this round.` : "";
    nodes.verdict.textContent =
      `${VERDICTS[level][first]} ${names[first]} ended ${closer}% closer than ${names[second].toLowerCase()}.${changed}`;
    nodes.verdict.hidden = false;
    render();
  }

  function scores() {
    return { you: typical(squared.you), idbd: typical(squared.idbd), lms: typical(squared.lms) };
  }

  function costs() {
    const truth = race.coefficients();
    return trustValues.map((t, s) => ({ cost: (t - truth[s]) ** 2, higher: truth[s] > t }));
  }

  function renderCosts() {
    costs().forEach(({ cost, higher }, s) => {
      const slider = sliders[s];
      const said = `you say: ${meaning(trustValues[s])}`;
      if (!hints) {
        slider.cost.textContent = said;
        slider.cost.className = "cost";
      } else if (cost < CLOSE) {
        slider.cost.textContent = `${said} \u2713 close`;
        slider.cost.className = "cost good";
      } else {
        slider.cost.textContent = `${said} \u00b7 ${higher ? "\u2191 try higher" : "\u2193 try lower"}`;
        slider.cost.className = "cost";
      }
    });
  }

  function render() {
    nodes.run.textContent = running ? "Pause" : finished ? "Play again" : "Start";
    nodes.run.disabled = Boolean(failure);
    nodes.count.textContent = `reading ${scored.toLocaleString("en-US")} of ${ROUND}`;
    const errors = scores();
    const leader =
      scored >= MIN_SCORED ? Object.keys(errors).reduce((a, b) => (errors[a] <= errors[b] ? a : b)) : null;
    for (const player of players) {
      const value = errors[player.key];
      player.value.textContent = value === null ? "--" : `${value.toFixed(2)} \u00b0C`;
      player.root.className = leader === player.key ? `score lead-${player.key}` : "score";
      player.captionNode.textContent = player.caption();
      const lead = finished ? "winner" : "in the lead";
      player.lead.textContent = leader === player.key ? lead : scored === 0 && !running ? "press Start" : "";
      player.lead.style.color = player.color;
    }
    renderCosts();
    renderChart();
    if (!nodes.learnedWrap.hidden) renderLearned();
  }

  function renderChart() {
    // The axis follows the two learners. A visitor far off the scale is drawn up to the
    // top and marked, not allowed to flatten everyone else.
    const learners = [...series.idbd, ...series.lms];
    const peak = Math.max(0.8, learners.length ? Math.max(...learners) : 0) * 1.25;
    const toY = (v) => (178 - (Math.min(v, peak) / peak) * 170).toFixed(1);
    // The first 40 units are a gutter for the degree labels. The x axis grows with the
    // round until it reaches its full length, so the lines fill the frame from the start.
    const span = Math.max(series.idbd.length, 60) - 1;
    const xAt = (i) => (40 + (i * 560) / span).toFixed(1);
    const line = (key) => series[key].map((v, i) => `${xAt(i)},${toY(v)}`).join(" ");
    let path = "";
    let drawing = false;
    series.you.forEach((v, i) => {
      if (v > peak) {
        drawing = false;
        return;
      }
      path += `${drawing ? "L" : "M"}${xAt(i)} ${toY(v)}`;
      drawing = true;
    });
    nodes.you.setAttribute("d", path);
    nodes.idbd.setAttribute("points", line("idbd"));
    nodes.lms.setAttribute("points", line("lms"));
    nodes.changes.setAttribute("d", changes.map((c, i) => (c ? `M${xAt(i)} 0V180` : "")).join(""));
    const y = toY(NOISE_FLOOR);
    for (const [key, value] of Object.entries({ x1: 40, x2: 600, y1: y, y2: y })) floor.setAttribute(key, value);
    floorLabel.setAttribute("y", String(Number(y) - 4));
    const offChart = series.you.length > 0 && series.you.at(-1) > peak;
    youOff.textContent = offChart ? "you: off the chart \u2191" : "";
    ticks.forEach(({ text, value }) => {
      text.setAttribute("y", String(Number(toY(value)) + 4));
      text.textContent = value <= peak ? `${value.toFixed(1)}\u00b0` : "";
    });
  }

  function renderLearned() {
    const alphas = race.idbdStepSizes();
    const weights = race.idbdWeights();
    // Bars share one fixed scale, so equal speeds look equal and the calm house does not
    // look like the busy one.
    const scale = 0.14;
    learned.forEach((item, s) => {
      const ratio = alphas[s] / START_SPEED;
      const pace =
        ratio > 1.3 ? "faster than at the start" : ratio < 0.7 ? "slower than at the start" : "about as fast as at the start";
      const said = meaning(weights[s]);
      item.weight.textContent = said === "ignore it" ? "thinks it barely matters right now" : `thinks it ${said}`;
      item.fill.style.width = `${Math.round(Math.min(1, alphas[s] / scale) * 100)}%`;
      item.speed.textContent = `learns about it ${pace} (speed ${alphas[s].toFixed(3)}, started at ${START_SPEED.toFixed(3)})`;
    });
  }

  function setRevealed(revealed) {
    nodes.learnedWrap.hidden = !revealed;
    nodes.reveal.setAttribute("aria-expanded", String(revealed));
    nodes.reveal.textContent = revealed ? "Hide" : "Show IDBD's trust and speeds";
    if (revealed && race) renderLearned();
  }

  nodes.run.addEventListener("click", () => (running ? stop() : start()));
  nodes.hints.addEventListener("click", () => {
    hints = !hints;
    nodes.hints.setAttribute("aria-pressed", String(hints));
    nodes.hints.textContent = hints ? "Hide hints" : "Show hints";
    renderCosts();
  });
  nodes.reset.addEventListener("click", reset);
  nodes.reveal.addEventListener("click", () => setRevealed(nodes.learnedWrap.hidden));
  for (const button of nodes.levels) {
    button.addEventListener("click", () => setLevel(button.dataset.level));
  }

  reset();

  return {
    state() {
      return {
        level,
        running,
        scored,
        scores: scores(),
        trust: [...trustValues],
        costs: costs().map((c) => c.cost),
        revealed: !nodes.learnedWrap.hidden,
        finished,
        hints,
        failure,
        stepSizes: Array.from(race.idbdStepSizes()),
        lmsStepSize: race.lmsStepSize(),
      };
    },
  };
}
