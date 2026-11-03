// "Try to keep up": the visitor's trust sliders against IDBD and a tuned fixed-step
// learner, both computed by the crate's HouseRace on the same readings.

const SEED = 20261103;
const WARMUP = 5000;
const READINGS_PER_TICK = 2;
const TICK_MS = 100;
const SCORE_WINDOW = 300;
const TRACE = 80;
/** Readings before a leader is named, so one lucky guess does not win. */
const MIN_SCORED = 50;
/** Sensors the visitor can set. The others count with trust 0. */
const PLAYER_SENSORS = [0, 1, 2, 3, 5, 9];

function mean(values) {
  return values.length ? values.reduce((a, b) => a + b, 0) / values.length : null;
}

function keep(list, value, size) {
  list.push(value);
  if (list.length > size) {
    list.shift();
  }
}

export function createGame(wasm, house, nodes) {
  let race = null;
  let running = false;
  let timer = null;
  let trustValues = new Array(house.sensors.length).fill(0);
  let scored = 0;
  let squared = { you: [], idbd: [], lms: [] };
  let trace = [];
  let failure = null;

  const sliderInputs = PLAYER_SENSORS.map((sensor) => {
    const id = `trust-${sensor}`;
    const row = document.createElement("div");
    row.className = "slider-row";
    const label = document.createElement("label");
    label.htmlFor = id;
    label.textContent = house.sensors[sensor];
    const input = document.createElement("input");
    Object.assign(input, { id, type: "range", min: "-1.5", max: "1.5", step: "0.1", value: "0" });
    const output = document.createElement("output");
    output.htmlFor = id;
    output.textContent = "0.0";
    input.addEventListener("input", () => {
      trustValues[sensor] = Number(input.value);
      output.textContent = trustValues[sensor].toFixed(1);
    });
    row.append(label, input, output);
    nodes.sliders.append(row);
    return { sensor, input, output };
  });

  const cards = [
    { key: "you", name: "You", color: "var(--you)", detail: "last 300 readings" },
    { key: "idbd", name: "IDBD", color: "var(--idbd)", detail: "last 300 readings" },
    { key: "lms", name: "Fixed speed", color: "var(--lms)", detail: `best single speed, ${house.lmsStepSize}` },
  ].map((card) => {
    const root = document.createElement("div");
    root.className = "score";
    const who = document.createElement("span");
    who.className = "who";
    const swatch = document.createElement("i");
    swatch.style.background = card.color;
    who.append(swatch, card.name);
    const value = document.createElement("span");
    value.className = "value";
    const note = document.createElement("span");
    note.className = "note";
    root.append(who, value, note);
    nodes.scores.append(root);
    return { ...card, root, value, note };
  });

  const learnedItems = house.sensors.map((name, sensor) => {
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

  function start() {
    if (running || failure) {
      return;
    }
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
    stop();
    if (race) {
      race.free();
    }
    race = new wasm.HouseRace(SEED);
    race.run(WARMUP);
    trustValues = new Array(house.sensors.length).fill(0);
    for (const slider of sliderInputs) {
      slider.input.value = "0";
      slider.output.textContent = "0.0";
    }
    scored = 0;
    squared = { you: [], idbd: [], lms: [] };
    trace = [];
    failure = null;
    nodes.failure.hidden = true;
    setRevealed(false);
    render();
  }

  function tick() {
    for (let i = 0; i < READINGS_PER_TICK; i += 1) {
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
      for (let s = 0; s < x.length; s += 1) {
        you += trustValues[s] * x[s];
      }
      const idbd = race.idbdPrediction();
      const lms = race.lmsPrediction();
      keep(squared.you, (y - you) ** 2, SCORE_WINDOW);
      keep(squared.idbd, (y - idbd) ** 2, SCORE_WINDOW);
      keep(squared.lms, (y - lms) ** 2, SCORE_WINDOW);
      keep(trace, { y, you, idbd, lms, changed: race.switched().length > 0 }, TRACE);
      scored += 1;
    }
    render();
  }

  function scores() {
    return { you: mean(squared.you), idbd: mean(squared.idbd), lms: mean(squared.lms) };
  }

  function render() {
    nodes.run.textContent = running ? "Pause" : "Start";
    nodes.run.disabled = Boolean(failure);
    nodes.count.textContent = `reading ${scored.toLocaleString("en-US")}`;
    const errors = scores();
    const ready = scored >= MIN_SCORED;
    const leader = ready
      ? Object.keys(errors).reduce((a, b) => (errors[a] <= errors[b] ? a : b))
      : null;
    for (const card of cards) {
      const value = errors[card.key];
      card.value.textContent = value === null ? "--" : value.toFixed(2);
      card.root.className = leader === card.key ? `score lead-${card.key}` : "score";
      card.note.textContent =
        leader === card.key ? "in the lead" : scored === 0 && !running ? "press Start" : card.detail;
      card.note.style.color = leader === card.key ? card.color : "";
    }
    const toY = (v) => (90 - Math.max(-4, Math.min(4, v)) * 21).toFixed(1);
    const xAt = (i) => ((i * 600) / (TRACE - 1)).toFixed(1);
    const line = (key) => trace.map((t, i) => `${xAt(i)},${toY(t[key])}`).join(" ");
    nodes.actual.setAttribute("points", line("y"));
    nodes.you.setAttribute("points", line("you"));
    nodes.idbd.setAttribute("points", line("idbd"));
    nodes.lms.setAttribute("points", line("lms"));
    nodes.changes.setAttribute(
      "d",
      trace.map((t, i) => (t.changed ? `M${xAt(i)} 0V180` : "")).join(""),
    );
    if (!nodes.learned.hidden) {
      renderLearned();
    }
  }

  function renderLearned() {
    const alphas = race.idbdStepSizes();
    const weights = race.idbdWeights();
    const scale = Math.max(...alphas);
    learnedItems.forEach((item, i) => {
      item.weight.textContent = `trust ${weights[i].toFixed(2)}`;
      item.fill.style.width = `${Math.round((alphas[i] / scale) * 100)}%`;
      item.speed.textContent = `speed ${alphas[i].toFixed(3)}`;
    });
  }

  function setRevealed(revealed) {
    nodes.learned.hidden = !revealed;
    nodes.reveal.setAttribute("aria-expanded", String(revealed));
    nodes.reveal.textContent = revealed ? "Hide" : "Show IDBD's speeds";
    if (revealed && race) {
      renderLearned();
    }
  }

  nodes.run.addEventListener("click", () => (running ? stop() : start()));
  nodes.reset.addEventListener("click", reset);
  nodes.reveal.addEventListener("click", () => setRevealed(nodes.learned.hidden));

  reset();

  return {
    state() {
      return {
        running,
        scored,
        scores: scores(),
        trust: [...trustValues],
        revealed: !nodes.learned.hidden,
        failure,
        stepSizes: Array.from(race.idbdStepSizes()),
      };
    },
  };
}
