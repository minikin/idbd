// "Play against it": the visitor's trust sliders against IDBD and a tuned fixed-step
// learner, both computed by the crate's HouseRace on the same readings. Level 1 is the
// still house, level 2 the switching house.

const SEED = 20261103;
const WARMUP = 5000;
const TICK_MS = 100;
const SCORE_WINDOW = 300;
const SMOOTH = 40;
const CHART = 600;
/** Readings before a leader is named, so one lucky guess does not win. */
const MIN_SCORED = 50;
/** A slider this close to the truth costs less than this per reading. */
const SPOT_ON = 0.005;
const NOISE_FLOOR = 0.25;

const LEVEL_NOTES = {
  still:
    "Nothing changes in this house. Match each sensor's true influence and you score about 0.25, the noise alone.",
  changing:
    "The sun, the oven and the front door switch between a strong and a weak effect about every two seconds. Try to keep up.",
};

const mean = (values) => (values.length ? values.reduce((a, b) => a + b, 0) / values.length : null);

function keep(list, value, size) {
  list.push(value);
  if (list.length > size) list.shift();
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

  const sliders = house.sensors.map((name, sensor) => {
    const id = `trust-${sensor}`;
    const wrap = document.createElement("div");
    wrap.className = "slider";
    const row = document.createElement("div");
    row.className = "slider-row";
    const label = document.createElement("label");
    label.htmlFor = id;
    label.textContent = name;
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
      renderCosts();
    });
    row.append(label, input, output);
    wrap.append(row, cost);
    nodes.sliders.append(wrap);
    return { input, output, cost };
  });

  const players = [
    { key: "you", name: "You", color: "var(--you)", caption: () => "your sliders" },
    { key: "idbd", name: "IDBD", color: "var(--idbd)", caption: () => "tunes a speed for each sensor" },
    {
      key: "lms",
      name: "Fixed speed",
      color: "var(--lms)",
      caption: () => `one speed for all, ${race.lmsStepSize()}, the best for this house`,
    },
  ].map((player) => {
    const root = document.createElement("div");
    root.className = "score";
    const who = document.createElement("span");
    who.className = "who";
    const swatch = document.createElement("i");
    swatch.style.background = player.color;
    who.append(swatch, player.name);
    const value = document.createElement("span");
    value.className = "value";
    const caption = document.createElement("span");
    caption.className = "note";
    const lead = document.createElement("span");
    lead.className = "lead";
    root.append(who, value, caption, lead);
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
    weight.className = "mono muted";
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

  const floor = document.createElementNS("http://www.w3.org/2000/svg", "line");
  floor.setAttribute("class", "floor-line");
  nodes.chart.insertBefore(floor, nodes.chart.firstChild);

  function newRace() {
    if (race) race.free();
    race = level === "still" ? wasm.HouseRace.still(SEED) : new wasm.HouseRace(SEED);
    race.run(WARMUP);
  }

  function start() {
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
    }
    scored = 0;
    squared = { you: [], idbd: [], lms: [] };
    recent = { you: [], idbd: [], lms: [] };
    series = { you: [], idbd: [], lms: [] };
    changes = [];
    failure = null;
    nodes.failure.hidden = true;
    nodes.levelNote.textContent = LEVEL_NOTES[level];
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
      keep(squared[key], misses[key], SCORE_WINDOW);
      keep(recent[key], misses[key], SMOOTH);
      keep(series[key], mean(recent[key]), CHART);
    }
    keep(changes, race.switched().length > 0, CHART);
    scored += 1;
    render();
  }

  function scores() {
    return { you: mean(squared.you), idbd: mean(squared.idbd), lms: mean(squared.lms) };
  }

  function costs() {
    const truth = race.coefficients();
    return trustValues.map((t, s) => ({ cost: (t - truth[s]) ** 2, higher: truth[s] > t }));
  }

  function renderCosts() {
    costs().forEach(({ cost, higher }, s) => {
      const slider = sliders[s];
      if (cost < SPOT_ON) {
        slider.cost.textContent = "spot on";
        slider.cost.className = "cost good";
      } else {
        slider.cost.textContent = `costs you ${cost.toFixed(2)} per reading, try ${higher ? "higher" : "lower"}`;
        slider.cost.className = "cost";
      }
    });
  }

  function render() {
    nodes.run.textContent = running ? "Pause" : "Start";
    nodes.run.disabled = Boolean(failure);
    nodes.count.textContent = `reading ${scored.toLocaleString("en-US")}`;
    const errors = scores();
    const leader =
      scored >= MIN_SCORED ? Object.keys(errors).reduce((a, b) => (errors[a] <= errors[b] ? a : b)) : null;
    for (const player of players) {
      const value = errors[player.key];
      player.value.textContent = value === null ? "--" : value.toFixed(2);
      player.root.className = leader === player.key ? `score lead-${player.key}` : "score";
      player.captionNode.textContent = player.caption();
      player.lead.textContent = leader === player.key ? "in the lead" : scored === 0 && !running ? "press Start" : "";
      player.lead.style.color = player.color;
    }
    renderCosts();
    renderChart();
    if (!nodes.learned.hidden) renderLearned();
  }

  function renderChart() {
    const peak = Math.max(1, ...series.idbd, ...series.lms) * 1.5;
    const toY = (v) => (178 - (Math.min(v, peak) / peak) * 170).toFixed(1);
    const xAt = (i) => ((i * 600) / (CHART - 1)).toFixed(1);
    const line = (key) => series[key].map((v, i) => `${xAt(i)},${toY(v)}`).join(" ");
    nodes.you.setAttribute("points", line("you"));
    nodes.idbd.setAttribute("points", line("idbd"));
    nodes.lms.setAttribute("points", line("lms"));
    nodes.changes.setAttribute("d", changes.map((c, i) => (c ? `M${xAt(i)} 0V180` : "")).join(""));
    const y = toY(NOISE_FLOOR);
    for (const [key, value] of Object.entries({ x1: 0, x2: 600, y1: y, y2: y })) floor.setAttribute(key, value);
  }

  function renderLearned() {
    const alphas = race.idbdStepSizes();
    const weights = race.idbdWeights();
    const scale = Math.max(...alphas);
    learned.forEach((item, s) => {
      item.weight.textContent = `trust ${weights[s].toFixed(2)}`;
      item.fill.style.width = `${Math.round((alphas[s] / scale) * 100)}%`;
      item.speed.textContent = `speed ${alphas[s].toFixed(3)}`;
    });
  }

  function setRevealed(revealed) {
    nodes.learned.hidden = !revealed;
    nodes.reveal.setAttribute("aria-expanded", String(revealed));
    nodes.reveal.textContent = revealed ? "Hide" : "Show IDBD's trust and speeds";
    if (revealed && race) renderLearned();
  }

  nodes.run.addEventListener("click", () => (running ? stop() : start()));
  nodes.reset.addEventListener("click", reset);
  nodes.reveal.addEventListener("click", () => setRevealed(nodes.learned.hidden));
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
        revealed: !nodes.learned.hidden,
        failure,
        stepSizes: Array.from(race.idbdStepSizes()),
        lmsStepSize: race.lmsStepSize(),
      };
    },
  };
}
