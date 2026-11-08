import { el } from "../src/dom.js";
import { createDial } from "./dial.js";

// "Family Assistant": the visitor marks the hours each sensor adds to the power meter,
// IDBD and a tuned fixed-speed learner watch three simulated days, and the page reveals
// the bars both drew. Every bar, found hour and score comes from the crate through
// FamilyDay and familyScore. The page only draws them.

/** Simulated readings per second of animation: three days in about seven seconds. */
const READINGS_PER_SECOND = 600;

const hourText = (hour) => `${String(hour).padStart(2, "0")}:00`;

const bitsOf = (from, until) => {
  let bits = 0;
  for (let hour = Math.min(from, until); hour < Math.max(from, until); hour += 1) bits |= 1 << hour;
  return bits >>> 0;
};

const count = (bits) => {
  let n = 0;
  for (let b = bits >>> 0; b; b &= b - 1) n += 1;
  return n;
};

/** The first and one-past-last hour of a contiguous set, or null when empty. */
function span(bits) {
  const hours = [];
  for (let h = 0; h < 24; h += 1) if (bits & (1 << h)) hours.push(h);
  return hours.length ? [hours[0], hours[hours.length - 1] + 1] : null;
}

function spanText(bits) {
  const s = span(bits);
  return s ? `${hourText(s[0])} to ${hourText(s[1])}` : "never";
}

/** Every run of consecutive hours in a set, in words, for screen readers. */
export function hoursText(bits) {
  const runs = [];
  let start = null;
  for (let h = 0; h <= 24; h += 1) {
    const on = h < 24 && Boolean(bits & (1 << h));
    if (on && start === null) start = h;
    if (!on && start !== null) {
      runs.push(`${hourText(start)} to ${hourText(h)}`);
      start = null;
    }
  }
  return runs.length ? runs.join(" and ") : "none";
}

/** What a set of guesses got right, in counts the page can put into words. */
export function tally(guesses, truth) {
  let sensorsFound = 0;
  let right = 0;
  let wrong = 0;
  let missed = 0;
  guesses.forEach((bits, i) => {
    if (truth[i] && bits & truth[i]) sensorsFound += 1;
    right += count(bits & truth[i]);
    wrong += count(bits & ~truth[i]);
    missed += count(truth[i] & ~bits);
  });
  return { sensorsFound, right, wrong, missed };
}

function tallyText({ sensorsFound, right, wrong }, relevant) {
  const hours = (n) => `${n} ${n === 1 ? "hour" : "hours"}`;
  const wrongText = wrong ? `, ${hours(wrong)} marked where nothing happens` : ", no wrong hours";
  return `${sensorsFound} of ${relevant} sensors found, ${hours(right)} right${wrongText}`;
}

export function createFamily(wasm, ui) {
  const home = JSON.parse(wasm.familyJson());
  const sensors = home.sensors;
  const truth = sensors.map((s) => s.hours >>> 0);
  const relevant = truth.filter(Boolean).length;
  const perDay = home.hours * home.readingsPerHour;
  const reducedMotion = window.matchMedia("(prefers-reduced-motion: reduce)");

  let seedIndex = 0;
  let day = null;
  let phase = "guess";
  let view = "idbd";
  let frame = null;
  let result = null;

  const lanes = sensors.map((sensor, i) => {
    const label = (end) => `${sensor.name}: matters ${end}`;
    const from = el("input", { type: "range", min: 0, max: 24, step: 1, value: 0, class: "from", "aria-label": label("from") });
    const until = el("input", { type: "range", min: 0, max: 24, step: 1, value: 0, class: "until", "aria-label": label("until") });
    const bars = el("div", { class: "lane-bars", "aria-hidden": "true" }, Array.from({ length: 24 }, () => el("i")));
    const truthBand = el("div", { class: "lane-truth", "aria-hidden": "true" });
    const you = el("div", { class: "lane-you", "aria-hidden": "true" });
    const note = el("span", { class: "lane-note" });
    const summary = el("span", { class: "sr-only" });
    const track = el("div", { class: "lane-track" }, [bars, truthBand, you, from, until]);
    const row = el("li", { class: "lane", "data-sensor": i }, [
      el("div", { class: "lane-name" }, [el("span", { class: "name", text: sensor.name }), note, summary]),
      track,
    ]);
    return { row, from, until, bars: [...bars.children], truthBand, you, note, summary };
  });
  ui.lanes.replaceChildren(...lanes.map((lane) => lane.row));
  const dial = createDial(ui.dial, sensors, home.days);

  const guesses = () => lanes.map((lane) => bitsOf(Number(lane.from.value), Number(lane.until.value)));

  function place(node, bits) {
    const s = span(bits);
    node.hidden = !s;
    if (s) {
      node.style.left = `${(s[0] / 24) * 100}%`;
      node.style.width = `${((s[1] - s[0]) / 24) * 100}%`;
    }
  }

  function syncLane(lane) {
    for (const input of [lane.from, lane.until]) input.setAttribute("aria-valuetext", hourText(Number(input.value)));
    const bits = bitsOf(Number(lane.from.value), Number(lane.until.value));
    place(lane.you, bits);
    lane.row.classList.toggle("empty", bits === 0);
    dial.guesses(guesses());
  }

  // The handles push each other instead of crossing, so "from" never names a later hour
  // than "until".
  for (const lane of lanes) {
    lane.from.addEventListener("input", () => {
      if (Number(lane.from.value) > Number(lane.until.value)) lane.until.value = lane.from.value;
      syncLane(lane);
    });
    lane.until.addEventListener("input", () => {
      if (Number(lane.until.value) < Number(lane.from.value)) lane.from.value = lane.until.value;
      syncLane(lane);
    });
    syncLane(lane);
  }

  function setClock(completed) {
    const dayNumber = Math.min(home.days, Math.floor(completed / perDay) + 1);
    const minute = Math.floor(((completed % perDay) / home.readingsPerHour) * 60);
    const time = `${String(Math.floor(minute / 60)).padStart(2, "0")}:${String(minute % 60).padStart(2, "0")}`;
    ui.clock.textContent =
      completed >= home.readings ? `Three days watched, ${home.readings.toLocaleString("en-US")} readings` : `Day ${dayNumber}, ${time}`;
    const fraction = completed >= home.readings ? 1 : (completed % perDay) / perDay;
    ui.lanes.style.setProperty("--now", String(fraction));
    const warming = completed < home.warmupReadings;
    dial.clock(fraction, dayNumber, time, warming ? "warm-up" : "");
  }

  /** Paints IDBD's bars so far on the dial. They exist only after the warm-up day. */
  function paintLive() {
    if (day.completed() <= home.warmupReadings) return;
    const bars = day.idbdBars();
    let peak = home.foundFloor;
    for (const value of bars) peak = Math.max(peak, Math.abs(value));
    dial.paint(bars, peak);
  }

  function setInputsDisabled(disabled) {
    for (const lane of lanes) {
      lane.from.disabled = disabled;
      lane.until.disabled = disabled;
    }
    ui.reset.disabled = disabled;
  }

  /** Stops a failed run and hands the controls back, so the visitor can try again. */
  function fail(message) {
    if (frame) cancelAnimationFrame(frame);
    frame = null;
    phase = "guess";
    ui.root.classList.remove("running");
    ui.stage.hidden = true;
    ui.stage.classList.remove("running");
    dial.reset();
    ui.clock.textContent = "";
    ui.lanes.style.removeProperty("--now");
    setInputsDisabled(false);
    ui.run.disabled = false;
    ui.failure.textContent = `The run stopped: ${message}. Your hours are kept. Run three days to try again.`;
    ui.failure.hidden = false;
  }

  function drawBars() {
    const values = view === "idbd" ? result.idbdBars : result.lmsBars;
    const found = view === "idbd" ? result.idbdFound : result.lmsFound;
    ui.root.dataset.view = view;
    lanes.forEach((lane, i) => {
      lane.bars.forEach((bar, h) => {
        const value = Math.abs(values[i * 24 + h]);
        bar.style.setProperty("--h", String(Math.min(1, value / result.peak)));
        bar.classList.toggle("on", Boolean(found[i] & (1 << h)));
      });
    });
    for (const button of ui.viewButtons) button.setAttribute("aria-pressed", String(button.dataset.view === view));
    const learner = view === "idbd" ? "IDBD" : "The fixed speed";
    const mine = guesses();
    lanes.forEach((lane, i) => {
      lane.summary.textContent = `. Your hours: ${hoursText(mine[i])}. ${learner} marks: ${hoursText(found[i])}.`;
    });
    dial.reveal({ values, found, peak: result.peak, truth, label: view === "idbd" ? "IDBD" : "Fixed speed" });
  }

  function scoreTile(who, swatch, score, words) {
    return el("div", { class: "score" }, [
      el("span", { class: "who" }, [el("i", { class: swatch }), who]),
      el("span", { class: "value", text: `${Math.round(score * 100)}%` }),
      el("span", { class: "note", text: words }),
    ]);
  }

  function reveal() {
    const mine = guesses();
    const idbdFound = Array.from(day.idbdFound());
    const lmsFound = Array.from(day.lmsFound());
    const idbdBars = Array.from(day.idbdBars());
    const lmsBars = Array.from(day.lmsBars());
    const speeds = Array.from(day.idbdSpeeds());
    const lmsSpeeds = Array.from(day.lmsSpeeds());
    const peak = Math.max(...idbdBars.map(Math.abs), ...lmsBars.map(Math.abs), 1e-9);
    result = {
      idbdBars,
      lmsBars,
      idbdFound,
      lmsFound,
      peak,
      scores: {
        you: wasm.familyScore(Uint32Array.from(mine)),
        idbd: wasm.familyScore(Uint32Array.from(idbdFound)),
        lms: wasm.familyScore(Uint32Array.from(lmsFound)),
      },
      speeds,
    };
    const mean = (pick) => {
      const list = speeds.filter((_, i) => pick(truth[i]));
      return list.reduce((a, b) => a + b, 0) / list.length;
    };
    const quiet = mean((t) => !t);
    lanes.forEach((lane, i) => {
      place(lane.truthBand, truth[i]);
      const when = truth[i] ? `matters ${spanText(truth[i])}` : "never matters";
      const pace = truth[i] ? `, IDBD learns ${(speeds[i] / quiet).toFixed(1)} times faster here` : "";
      lane.note.textContent = `${when}${pace}`;
    });
    ui.root.classList.add("revealed");
    ui.view.hidden = false;
    ui.reset.hidden = true;
    ui.hint.hidden = true;
    ui.key.hidden = false;
    drawBars();
    const { scores } = result;
    ui.scores.replaceChildren(
      scoreTile("You", "swatch-you", scores.you, tallyText(tally(mine, truth), relevant)),
      scoreTile("IDBD", "swatch-idbd", scores.idbd, tallyText(tally(idbdFound, truth), relevant)),
      scoreTile("Fixed speed", "swatch-lms", scores.lms, tallyText(tally(lmsFound, truth), relevant)),
    );
    const ratio = mean(Boolean) / quiet;
    ui.mechanism.textContent =
      `IDBD watched twelve sensors and the meter for three days and was never told which sensors matter. ` +
      `By the end it learned about the four that do ${ratio.toFixed(1)} times faster than about the eight that never do. ` +
      `The fixed-speed learner, tuned for this home, uses one step size for every sensor, so how fast it learns about a sensor ` +
      `depends only on how large that sensor's readings are: ${(Math.max(...lmsSpeeds) / Math.min(...lmsSpeeds)).toFixed(0)} times ` +
      `faster on the sensors with the largest readings than on those with the smallest, whether they matter or not.`;
    ui.result.hidden = false;
    ui.stage.hidden = false;
    ui.stage.classList.remove("running");
    ui.resultTitle.focus();
  }

  function finish() {
    frame = null;
    phase = "done";
    setClock(day.completed());
    ui.run.hidden = true;
    reveal();
  }

  function step(start, startedAt) {
    return (now) => {
      const target = Math.min(home.readings, Math.floor(((now - startedAt) / 1000) * READINGS_PER_SECOND));
      try {
        day.run(Math.max(0, target - day.completed()));
      } catch (error) {
        fail(day.failureMessage() ?? error.message);
        return;
      }
      setClock(day.completed());
      paintLive();
      if (day.completed() >= home.readings) finish();
      else frame = requestAnimationFrame(step(start, startedAt));
    };
  }

  function run() {
    if (phase !== "guess") return;
    phase = "running";
    ui.failure.hidden = true;
    setInputsDisabled(true);
    ui.run.disabled = true;
    day?.free();
    day = null;
    try {
      day = new wasm.FamilyDay(home.seeds[seedIndex]);
      if (reducedMotion.matches) day.run(home.readings);
    } catch (error) {
      fail(day?.failureMessage() ?? error.message);
      return;
    }
    if (reducedMotion.matches) {
      finish();
      return;
    }
    ui.root.classList.add("running");
    ui.stage.hidden = false;
    ui.stage.classList.add("running");
    frame = requestAnimationFrame((now) => step(0, now)(now));
  }

  function again() {
    if (frame) cancelAnimationFrame(frame);
    frame = null;
    seedIndex = (seedIndex + 1) % home.seeds.length;
    phase = "guess";
    result = null;
    ui.root.classList.remove("revealed", "running");
    ui.view.hidden = true;
    ui.result.hidden = true;
    ui.stage.hidden = true;
    ui.stage.classList.remove("running");
    dial.reset();
    ui.reset.hidden = false;
    ui.hint.hidden = false;
    ui.key.hidden = true;
    ui.run.hidden = false;
    ui.run.disabled = false;
    ui.clock.textContent = "";
    ui.lanes.style.removeProperty("--now");
    setInputsDisabled(false);
    lanes.forEach((lane) => {
      lane.note.textContent = "";
      lane.summary.textContent = "";
      lane.truthBand.hidden = true;
    });
    ui.failure.hidden = true;
    ui.run.focus();
  }

  function clear() {
    for (const lane of lanes) {
      lane.from.value = "0";
      lane.until.value = "0";
      syncLane(lane);
    }
  }

  ui.run.addEventListener("click", run);
  ui.reset.addEventListener("click", clear);
  ui.again.addEventListener("click", again);
  for (const button of ui.viewButtons) {
    button.addEventListener("click", () => {
      view = button.dataset.view;
      if (result) drawBars();
    });
  }
  lanes.forEach((lane) => (lane.truthBand.hidden = true));

  return {
    /** Sets one lane's guess, for tests: hours `from` up to `until`. */
    guess(sensor, from, until) {
      lanes[sensor].from.value = String(from);
      lanes[sensor].until.value = String(until);
      syncLane(lanes[sensor]);
    },
    state: () => ({
      phase,
      view,
      seed: home.seeds[seedIndex],
      completed: day ? day.completed() : 0,
      guesses: guesses(),
      truth,
      scores: result?.scores ?? null,
      idbdFound: result?.idbdFound ?? null,
      lmsFound: result?.lmsFound ?? null,
    }),
  };
}
