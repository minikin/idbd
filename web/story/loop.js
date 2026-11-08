// "What IDBD does": the four steps of one update, walked through on live readings from a
// FamilyDay. The highlight moves from step to step, and each card shows what that step did
// on the current reading. The cards count sensors and never name them, so this chapter does
// not give away which sensors matter before the game.

import { inWords, missInWords } from "./slow.js";

const SEED = 1;
/** The walk starts after one day, when IDBD has learned enough for its guesses to mean something. */
const START = 1440;
const STEP_MS = 1800;
/** The step a still page highlights: tuning the speeds, the step that is IDBD. */
const IDBD_STEP = 2;

const plural = (n, one, many) => `${n} ${n === 1 ? one : many}`;
/** The value `inWords` prints, so a difference of two printed values matches the printed miss. */
const cents = (v) => Math.sign(v) * Number(Math.abs(v).toFixed(2));

/** What each step did on one reading, in the words the cards show. */
export function stepTexts(before, after) {
  const up = after.speeds.filter((a, s) => a > before.speeds[s]).length;
  const down = after.speeds.filter((a, s) => a < before.speeds[s]).length;
  const moves = after.weights.map((w, s) => Math.abs(w - before.weights[s]));
  const moved = moves.filter((m) => m > 0).length;
  const largest = Math.max(...moves);
  return [
    `IDBD guesses ${inWords(after.guess)}.`,
    // The miss is taken between the two rounded values, so the line adds up as printed.
    `The meter reads ${inWords(after.target)}, so the guess is ${missInWords(cents(after.target), cents(after.guess))}.`,
    `${plural(up, "speed", "speeds")} went up and ${down} went down.`,
    moved === 0
      ? "No trust moved."
      : `${moved === moves.length ? `All ${moved} trusts` : plural(moved, "trust", "trusts")} moved, the largest by ${largest.toFixed(3)} kW per swing.`,
  ];
}

export function createLoop(wasm, home, nodes, { clockText, reducedMotion }) {
  let day = null;
  let texts = ["", "", "", ""];
  let active = IDBD_STEP;
  let paused = reducedMotion.matches;
  let shown = false;
  let timer = null;

  function restart() {
    if (day) day.free();
    day = new wasm.FamilyDay(SEED);
    day.run(START);
  }

  function snapshot() {
    return { speeds: Array.from(day.idbdSpeeds()), weights: Array.from(day.idbdWeights()) };
  }

  function nextReading() {
    if (day.completed() >= home.readings) restart();
    const before = snapshot();
    day.step();
    texts = stepTexts(before, { ...snapshot(), guess: day.idbdPrediction(), target: day.target() });
    nodes.clock.textContent = `The reading at ${clockText(day.completed())}`;
    nodes.live.forEach((node, i) => {
      node.textContent = texts[i];
    });
  }

  function highlight(step) {
    active = step;
    nodes.cards.forEach((card, i) => {
      card.classList.toggle("highlight", i === step);
      if (i === step) card.setAttribute("aria-current", "step");
      else card.removeAttribute("aria-current");
    });
  }

  function tick() {
    const step = (active + 1) % nodes.cards.length;
    if (step === 0) nextReading();
    highlight(step);
  }

  function sync() {
    const run = shown && !paused && !document.hidden;
    if (run && timer === null) timer = setInterval(tick, STEP_MS);
    else if (!run && timer !== null) {
      clearInterval(timer);
      timer = null;
    }
    nodes.pause.textContent = paused ? "Play" : "Pause";
    nodes.pause.setAttribute("aria-label", paused ? "Play the four steps" : "Pause the four steps");
  }

  restart();
  nextReading();
  highlight(IDBD_STEP);
  new IntersectionObserver(([entry]) => {
    shown = entry.isIntersecting;
    sync();
  }).observe(nodes.list);
  document.addEventListener("visibilitychange", sync);
  nodes.pause.addEventListener("click", () => {
    paused = !paused;
    sync();
  });
  reducedMotion.addEventListener("change", () => {
    if (!reducedMotion.matches) return;
    paused = true;
    sync();
  });
  sync();

  return {
    state: () => ({
      completed: day.completed(),
      active,
      running: timer !== null,
      paused,
      texts: [...texts],
    }),
  };
}
