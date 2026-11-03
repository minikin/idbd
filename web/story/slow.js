// Slow motion: one reading at a time from a fresh HouseRace, with every change IDBD makes
// read from the crate before and after the update.

const SEED = 3;
const RUN = 5000;
const HISTORY = 600;

const fmt = (v, digits = 2) => `${v >= 0 ? "+" : "−"}${Math.abs(v).toFixed(digits)}`;

function sparkline(values) {
  if (values.length < 2) {
    return "";
  }
  const max = Math.max(...values);
  const min = Math.min(...values);
  const span = max - min || 1;
  return values
    .map((v, i) => `${((i * 120) / (values.length - 1)).toFixed(1)},${(22 - ((v - min) / span) * 20).toFixed(1)}`)
    .join(" ");
}

export function createSlowMotion(wasm, house, nodes) {
  let race = null;
  let history = [];
  let last = null;

  const rows = house.sensors.map((name, sensor) => {
    const row = document.createElement("tr");
    if (sensor < house.relevant) row.className = "relevant";
    const head = document.createElement("th");
    head.scope = "row";
    head.textContent = name;
    const cells = ["reading", "trust", "speed"].map(() => document.createElement("td"));
    const sparkCell = document.createElement("td");
    const chart = document.createElementNS("http://www.w3.org/2000/svg", "svg");
    chart.setAttribute("viewBox", "0 0 120 24");
    chart.setAttribute("aria-hidden", "true");
    const line = document.createElementNS("http://www.w3.org/2000/svg", "polyline");
    chart.append(line);
    sparkCell.append(chart);
    row.append(head, ...cells, sparkCell);
    nodes.rows.append(row);
    return { reading: cells[0], trust: cells[1], speed: cells[2], line };
  });

  function restart() {
    if (race) race.free();
    race = new wasm.HouseRace(SEED);
    const alphas = Array.from(race.idbdStepSizes());
    history = alphas.map((a) => [a]);
    last = null;
    render();
  }

  function advance(count) {
    let before = null;
    for (let i = 0; i < count; i += 1) {
      before = { alphas: Array.from(race.idbdStepSizes()), weights: Array.from(race.idbdWeights()) };
      race.step();
      race.idbdStepSizes().forEach((a, s) => {
        history[s].push(a);
        if (history[s].length > HISTORY) history[s].shift();
      });
    }
    last = {
      before,
      alphas: Array.from(race.idbdStepSizes()),
      weights: Array.from(race.idbdWeights()),
      features: Array.from(race.features()),
      target: race.target(),
      guess: race.idbdPrediction(),
    };
    render();
  }

  function render() {
    nodes.reading.textContent = race.completed().toLocaleString("en-US");
    rows.forEach((row, s) => row.line.setAttribute("points", sparkline(history[s])));
    if (!last) {
      for (const node of [nodes.guess, nodes.actual, nodes.miss]) node.textContent = "-";
      nodes.summary.textContent =
        "Press Next reading to start. Every sensor begins with trust 0 and the same speed, 0.0500.";
      rows.forEach((row, s) => {
        row.reading.textContent = "-";
        row.trust.textContent = "+0.00";
        row.speed.textContent = history[s][0].toFixed(4);
      });
      return;
    }
    const miss = last.target - last.guess;
    nodes.guess.textContent = `${fmt(last.guess)} \u00b0C`;
    nodes.actual.textContent = `${fmt(last.target)} \u00b0C`;
    nodes.miss.textContent = `${fmt(miss)} \u00b0C`;

    let biggest = 0;
    let up = 0;
    let down = 0;
    rows.forEach((row, s) => {
      const w0 = last.before.weights[s];
      const w1 = last.weights[s];
      const a0 = last.before.alphas[s];
      const a1 = last.alphas[s];
      if (Math.abs(w1 - w0) > Math.abs(last.weights[biggest] - last.before.weights[biggest])) biggest = s;
      const percent = ((a1 - a0) / a0) * 100;
      const flat = Math.abs(percent) < 0.05;
      if (!flat && a1 > a0) up += 1;
      if (!flat && a1 < a0) down += 1;
      row.reading.textContent = fmt(last.features[s]);
      row.trust.textContent = `${fmt(w0)} \u2192 ${fmt(w1)}`;
      row.speed.textContent = flat
        ? `${a0.toFixed(4)} \u2192 ${a1.toFixed(4)} barely moved`
        : `${a0.toFixed(4)} \u2192 ${a1.toFixed(4)} ${a1 > a0 ? "\u2191" : "\u2193"} ${fmt(percent, 1)}%`;
      row.speed.className = flat ? "flat" : a1 > a0 ? "up" : "down";
    });
    const change = last.weights[biggest] - last.before.weights[biggest];
    nodes.summary.textContent =
      `IDBD guessed ${fmt(last.guess)} \u00b0C, the room changed by ${fmt(last.target)} \u00b0C, so it missed by ` +
      `${fmt(miss)} \u00b0C. It moved its trust in the ${house.sensors[biggest]} the most, by ${fmt(change, 3)}. ` +
      `${up} speeds went up and ${down} went down. A speed goes up when that sensor's correction points ` +
      `the same way as its recent ones.`;
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
        last: last && { guess: last.guess, target: last.target },
      };
    },
  };
}
