import { SENSOR_COLORS } from "./house.js";

// "Play against it": the visitor's sliders against IDBD and a tuned fixed-step
// learner, both computed by the crate's HouseRace on the same readings. Level 1 is the
// still house, level 2 the switching house.

const SEED = 20261103;
const WARMUP = 5000;
const TICK_MS = 100;
/** One round is 600 readings, a minute at 10 readings a second. Scores cover the round. */
const ROUND = 600;
const SMOOTH = 100;
const CHART = ROUND;
/** Readings before a leader is named, so one lucky guess does not win. */
const MIN_SCORED = 50;
/** IDBD's starting step size, the reference for "faster" and "slower". */
const START_SPEED = 0.05;
/** The noise alone keeps even a perfect guess this far off, in degrees: sqrt(0.25). */
const NOISE_FLOOR = 0.5;

const LEVEL_NOTES = {
  changing:
    "The sun, the oven and the front door switch between a strong and a weak effect about every two seconds. Keeping up with that is what IDBD is for. Try to beat it.",
  still:
    "Nothing changes in this house, so one well-chosen fixed speed is enough, and it usually edges out IDBD here. Set every slider right and you can beat both.",
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
    you: "You win. You found how much each sensor matters, and in a calm house that never changes.",
    idbd: "IDBD wins this round, narrowly. In a calm house the fixed speed usually edges ahead.",
    lms: "The fixed speed wins. That is expected: in a calm house one well-chosen speed is enough, and IDBD pays a little for staying ready to change.",
  },
  changing: {
    you: "You beat both learners while the house kept changing. That is rare.",
    idbd: "IDBD wins. It kept up by learning fast only about the sun, the oven and the front door. Now try Level 2, a calm house, where that edge does not help.",
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


/**
 * Accuracy from 0 to 1 against a fixed scale for the house. 1 misses only by the noise.
 * 0 is the long-run error of always guessing "no change" in that house, which follows
 * from its definition: each relevant sensor's effect squared, averaged over its strong
 * and weak level when the house switches, plus the noise.
 */
function baseline(house, level) {
  const effects = house.levels.reduce(
    (sum, [strong, weak]) => sum + (level === "still" ? strong ** 2 : (strong ** 2 + weak ** 2) / 2),
    0,
  );
  return effects + NOISE_FLOOR ** 2;
}

function accuracy(squaredMisses, nothing) {
  if (!squaredMisses.length) return null;
  const mean = squaredMisses.reduce((a, b) => a + b, 0) / squaredMisses.length;
  const floor = NOISE_FLOOR ** 2;
  return Math.max(0, Math.min(1, (nothing - mean) / (nothing - floor)));
}

export function createGame(wasm, house, nodes) {
  let level = "changing";
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

  // Five stops per sensor. "A lot" is the sensor's strong effect and "a little" its weak
  // one; the three sensors that never matter use 1 and 0.2. In the calm house the right
  // stops are exactly the truth, and in the busy house a player who spots a switch can
  // follow it by hand.
  const STOPS = ["cools a lot", "cools a little", "ignore", "warms a little", "warms a lot"];
  const effect = (sensor, mark) => {
    if (mark === 0) return 0;
    const [strong, weak] = sensor < house.relevant ? house.levels[sensor].map(Math.abs) : [1, 0.2];
    return Math.sign(mark) * (Math.abs(mark) === 2 ? strong : weak);
  };
  let marks = house.sensors.map(() => 0);

  // The stop names sit once above the rails, each centred over its stop.
  const scale = document.createElement("div");
  scale.className = "stop-scale";
  scale.setAttribute("aria-hidden", "true");
  const stopTicks = document.createElement("div");
  stopTicks.className = "stop-ticks";
  STOPS.forEach((stop, i) => {
    const tick = document.createElement("span");
    tick.textContent = stop;
    tick.style.left = `${i * 25}%`;
    stopTicks.append(tick);
  });
  scale.append(document.createElement("span"), stopTicks);
  nodes.sliders.append(scale);

  const sliders = house.sensors.map((name, sensor) => {
    const id = `trust-${sensor}`;
    const row = document.createElement("div");
    row.className = "stop-row";
    const label = document.createElement("label");
    label.htmlFor = id;
    const dot = document.createElement("i");
    dot.className = "sensor-dot";
    dot.style.background = SENSOR_COLORS[sensor];
    label.append(dot, name);
    const input = document.createElement("input");
    Object.assign(input, { id, type: "range", min: "-2", max: "2", step: "1", value: "0" });
    input.className = "stop-input";
    const output = document.createElement("output");
    output.htmlFor = id;
    input.addEventListener("input", () => setMark(sensor, Number(input.value)));
    row.append(label, input, output);
    nodes.sliders.append(row);
    return { input, output };
  });

  function setMark(sensor, mark) {
    marks[sensor] = mark;
    trustValues[sensor] = effect(sensor, mark);
    const { input, output } = sliders[sensor];
    input.value = String(mark);
    input.setAttribute("aria-valuetext", STOPS[mark + 2]);
    input.style.setProperty("--lo", `${Math.min(50, (mark + 2) * 25)}%`);
    input.style.setProperty("--hi", `${Math.max(50, (mark + 2) * 25)}%`);
    output.textContent = STOPS[mark + 2];
    output.dataset.mark = mark > 0 ? "warms" : mark < 0 ? "cools" : "ignore";
  }

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
    prefix.textContent = "accuracy, last 10 seconds";
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
  floorLabel.textContent = "as good as possible";
  const ticks = [0, 0.5, 1].map((value) => {
    const text = document.createElementNS(SVG, "text");
    text.setAttribute("class", "chart-label");
    text.setAttribute("x", "2");
    return { text, value };
  });
  nodes.chart.append(floorLabel, ...ticks.map((t) => t.text));

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
    marks = house.sensors.map(() => 0);
    house.sensors.forEach((_, sensor) => setMark(sensor, 0));
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
      keep(series[key], accuracy(recent[key], baseline(house, level)) ?? 0, CHART);
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
    const [first] = order;
    const names = { you: "You", idbd: "IDBD", lms: "Fixed speed" };
    const acc = accuracies();
    const table = order.map((key) => `${names[key]} ${Math.round((acc[key] ?? 0) * 100)}%`).join(", ");
    const switches = changes.filter(Boolean).length;
    const changed = level === "changing" ? ` What matters changed ${switches} times this round.` : "";
    const yours = acc.you ?? 0;
    const stars = yours >= 0.9 ? 3 : yours >= 0.7 ? 2 : yours >= 0.4 ? 1 : 0;
    const starText = `${"\u2605".repeat(stars)}${"\u2606".repeat(3 - stars)}`;
    nodes.verdict.textContent = `${starText}  ${VERDICTS[level][first]} Accuracy over the round: ${table}.${changed}`;
    nodes.verdict.hidden = false;
    render();
  }

  /** Typical miss in degrees, per player. */
  function scores() {
    return { you: typical(squared.you), idbd: typical(squared.idbd), lms: typical(squared.lms) };
  }

  /** Accuracy over the round, per player. Higher is better. */
  function accuracies() {
    return {
      you: accuracy(squared.you, baseline(house, level)),
      idbd: accuracy(squared.idbd, baseline(house, level)),
      lms: accuracy(squared.lms, baseline(house, level)),
    };
  }

  function render() {
    nodes.run.textContent = running ? "Pause" : finished ? "Play again" : "Start";
    nodes.run.disabled = Boolean(failure);
    nodes.count.textContent = `reading ${scored.toLocaleString("en-US")} of ${ROUND}`;
    const errors = scores();
    const leader =
      scored >= MIN_SCORED ? Object.keys(errors).reduce((a, b) => (errors[a] <= errors[b] ? a : b)) : null;
    const acc = {
      you: accuracy(recent.you, baseline(house, level)),
      idbd: accuracy(recent.idbd, baseline(house, level)),
      lms: accuracy(recent.lms, baseline(house, level)),
    };
    for (const player of players) {
      const value = errors[player.key];
      const share = acc[player.key];
      player.value.textContent = share === null ? "--" : `${Math.round(share * 100)}%`;
      player.root.className = leader === player.key ? `score lead-${player.key}` : "score";
      player.captionNode.textContent = player.caption();
      const lead = finished ? "winner" : "in the lead";
      player.lead.textContent = leader === player.key ? lead : scored === 0 && !running ? "press Start" : "";
      player.lead.style.color = player.color;
    }
    renderChart();
    if (!nodes.learnedWrap.hidden) renderLearned();
  }

  function renderChart() {
    // Accuracy over the last few readings, 0 to 100 percent. The first 40 units are a
    // gutter for the axis labels. The x axis grows with the round until it reaches its
    // full length, so the lines fill the frame from the start.
    const toY = (v) => (176 - v * 166).toFixed(1);
    const span = Math.max(series.idbd.length, 60) - 1;
    const xAt = (i) => (40 + (i * 560) / span).toFixed(1);
    const line = (key) => series[key].map((v, i) => `${xAt(i)},${toY(v)}`).join(" ");
    nodes.you.setAttribute("d", series.you.length ? `M${line("you").replaceAll(" ", "L")}` : "");
    nodes.idbd.setAttribute("points", line("idbd"));
    nodes.lms.setAttribute("points", line("lms"));
    nodes.changes.setAttribute("d", changes.map((c, i) => (c ? `M${xAt(i)} 0V180` : "")).join(""));
    const y = toY(1);
    for (const [key, value] of Object.entries({ x1: 40, x2: 600, y1: y, y2: y })) floor.setAttribute(key, value);
    floorLabel.setAttribute("y", String(Number(y) + 12));
    ticks.forEach(({ text, value }) => {
      text.setAttribute("y", String(Number(toY(value)) + 4));
      text.textContent = `${Math.round(value * 100)}%`;
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
        accuracy: accuracies(),
        trust: [...trustValues],
        marks: [...marks],
        revealed: !nodes.learnedWrap.hidden,
        finished,
        failure,
        stepSizes: Array.from(race.idbdStepSizes()),
        lmsStepSize: race.lmsStepSize(),
      };
    },
  };
}
