import { svg } from "../src/dom.js";

// The Family Assistant clock: one ring per sensor around a 24-hour dial, midnight at the
// top. A hand sweeps each day while the learners watch and paints IDBD's bars as they
// grow, and the reveal fills each ring with the true hours, the visitor's hours and the
// learner's bars. It draws values from the crate and adds none of its own.

const SIZE = 520;
const C = SIZE / 2;
const INNER = 62;
const RING = 11;
const GAP = 3;
/** Hours of fading wake behind the hand, drawn as this many slices. */
const TRAIL_HOURS = 1.8;
const TRAIL_SLICES = 18;

const angle = (hour) => (hour / 24) * 2 * Math.PI - Math.PI / 2;
const point = (r, hour) => [C + r * Math.cos(angle(hour)), C + r * Math.sin(angle(hour))];

/** An annular sector from hour `a` to hour `b` between radii `r0` and `r1`. */
function sector(r0, r1, a, b) {
  const [x0, y0] = point(r1, a);
  const [x1, y1] = point(r1, b);
  const [x2, y2] = point(r0, b);
  const [x3, y3] = point(r0, a);
  const large = b - a > 12 ? 1 : 0;
  const f = (n) => n.toFixed(2);
  return `M${f(x0)} ${f(y0)}A${r1} ${r1} 0 ${large} 1 ${f(x1)} ${f(y1)}L${f(x2)} ${f(y2)}A${r0} ${r0} 0 ${large} 0 ${f(x3)} ${f(y3)}Z`;
}

function arc(r, a, b) {
  const [x0, y0] = point(r, a);
  const [x1, y1] = point(r, b);
  const large = b - a > 12 ? 1 : 0;
  return `M${x0.toFixed(2)} ${y0.toFixed(2)}A${r} ${r} 0 ${large} 1 ${x1.toFixed(2)} ${y1.toFixed(2)}`;
}

function spans(bits) {
  const out = [];
  let start = null;
  for (let h = 0; h <= 24; h += 1) {
    const on = h < 24 && Boolean(bits & (1 << h));
    if (on && start === null) start = h;
    if (!on && start !== null) {
      out.push([start, h]);
      start = null;
    }
  }
  return out;
}

export function createDial(root, sensors, days) {
  const rings = sensors.map((_, i) => {
    const r0 = INNER + i * (RING + GAP);
    const r1 = r0 + RING;
    const cells = Array.from({ length: 24 }, (_, h) =>
      svg("path", { d: sector(r0, r1, h + 0.06, h + 0.94), class: `dial-cell${h < 6 || h >= 21 ? " night" : ""}` }),
    );
    const truth = svg("g", { class: "dial-truth" });
    const you = svg("g", { class: "dial-you" });
    return { r0, r1, cells, truth, you };
  });
  const outer = INNER + sensors.length * (RING + GAP);
  const labels = [0, 6, 12, 18].map((h) => {
    const [x, y] = point(outer + 16, h);
    return svg("text", { x: x.toFixed(1), y: (y + 4).toFixed(1), class: "dial-label", "text-anchor": "middle", text: `${String(h).padStart(2, "0")}:00` });
  });
  const ticks = Array.from({ length: 24 }, (_, h) => {
    const [x0, y0] = point(outer + 1, h);
    const [x1, y1] = point(outer + (h % 6 ? 4 : 7), h);
    return svg("line", { x1: x0.toFixed(2), y1: y0.toFixed(2), x2: x1.toFixed(2), y2: y1.toFixed(2), class: "dial-tick" });
  });
  const rim = svg("circle", { cx: C, cy: C, r: outer + 1, class: "dial-rim" });
  // The wake is drawn once with the hand at midnight and turned with it.
  const slice = TRAIL_HOURS / TRAIL_SLICES;
  const trail = svg(
    "g",
    { class: "dial-trail" },
    Array.from({ length: TRAIL_SLICES }, (_, k) =>
      svg("path", {
        d: sector(INNER - 2, outer + 1, -(k + 1) * slice, -k * slice + 0.01),
        style: `opacity:${((1 - k / TRAIL_SLICES) ** 2).toFixed(3)}`,
      }),
    ),
  );
  const tip = point(outer + 4, 0);
  const sweep = svg("g", { class: "dial-sweep" }, [
    trail,
    svg("line", { x1: C, y1: C - INNER + 6, x2: tip[0], y2: tip[1], class: "dial-hand" }),
    svg("circle", { cx: tip[0], cy: tip[1], r: 3.5, class: "dial-tip" }),
  ]);
  const dayText = svg("text", { x: C, y: C - 8, class: "dial-day", "text-anchor": "middle" });
  const timeText = svg("text", { x: C, y: C + 12, class: "dial-time", "text-anchor": "middle" });
  const pips = Array.from({ length: days }, (_, d) =>
    svg("circle", { cx: C + (d - (days - 1) / 2) * 14, cy: C + 30, r: 3.5, class: "dial-pip" }),
  );
  const pipGroup = svg("g", { class: "dial-pips" }, pips);
  // Side margin for the 06:00 and 18:00 labels, which sit beyond the outer ring.
  root.setAttribute("viewBox", `-30 0 ${SIZE + 60} ${SIZE}`);
  root.replaceChildren(
    ...rings.flatMap((ring) => [...ring.cells, ring.truth, ring.you]),
    rim,
    ...ticks,
    ...labels,
    sweep,
    dayText,
    timeText,
    pipGroup,
  );

  let shownDay = 0;

  function drawSpans(group, bits, r) {
    group.replaceChildren(...spans(bits).map(([a, b]) => svg("path", { d: arc(r, a, b) })));
  }

  /** Restarts the rim's pulse, which marks the turn of a day. */
  function pulse() {
    root.classList.remove("turn");
    root.getBoundingClientRect();
    root.classList.add("turn");
  }

  return {
    /**
     * Moves the hand to `fraction` of day `day` (1-based), names the day and time and
     * fills the day pips. `note` follows the time, for example during the warm-up.
     */
    clock(fraction, day, time, note = "") {
      sweep.setAttribute("transform", `rotate(${(fraction * 360).toFixed(2)} ${C} ${C})`);
      sweep.style.display = "";
      pipGroup.style.display = "";
      dayText.textContent = `Day ${day}`;
      timeText.textContent = note ? `${time}, ${note}` : time;
      pips.forEach((pip, d) => {
        pip.classList.toggle("done", d + 1 < day);
        pip.classList.toggle("now", d + 1 === day);
      });
      if (shownDay && day !== shownDay) pulse();
      shownDay = day;
    },
    /** Paints IDBD's bars so far on the rings, `peak` being full strength. */
    paint(values, peak) {
      rings.forEach((ring, i) => {
        ring.cells.forEach((cell, h) => {
          const strength = Math.min(1, Math.abs(values[i * 24 + h]) / peak);
          cell.style.setProperty("--h", strength.toFixed(3));
          cell.classList.add("live");
        });
      });
    },
    /** Draws the visitor's hours on every ring. */
    guesses(bits) {
      rings.forEach((ring, i) => drawSpans(ring.you, bits[i], ring.r0 - 1));
    },
    /** Fills the rings with a learner's bars and the true hours, and parks the hand. */
    reveal({ values, found, peak, truth, label }) {
      rings.forEach((ring, i) => {
        ring.cells.forEach((cell, h) => {
          const strength = Math.min(1, Math.abs(values[i * 24 + h]) / peak);
          cell.style.setProperty("--h", strength.toFixed(3));
          cell.classList.toggle("on", Boolean(found[i] & (1 << h)));
          cell.classList.remove("live");
          cell.classList.add("shown");
        });
        ring.truth.replaceChildren(
          ...spans(truth[i]).map(([a, b]) => svg("path", { d: sector(ring.r0 - 1.5, ring.r1 + 1.5, a, b) })),
        );
      });
      sweep.style.display = "none";
      pipGroup.style.display = "none";
      dayText.textContent = label;
      timeText.textContent = "after three days";
    },
    /** Clears the reveal for a new run. */
    reset() {
      for (const ring of rings) {
        for (const cell of ring.cells) {
          cell.classList.remove("on", "shown", "live");
          cell.style.removeProperty("--h");
        }
        ring.truth.replaceChildren();
      }
      sweep.style.display = "none";
      pipGroup.style.display = "none";
      root.classList.remove("turn");
      shownDay = 0;
      dayText.textContent = "";
      timeText.textContent = "";
    },
  };
}
