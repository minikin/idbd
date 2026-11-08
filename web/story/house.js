// Where each sensor sits in the home, and the 2D drawing that shows the learned speeds.
// House coordinates: x runs along the right-hand back wall (0 to 8), y along the
// left-hand back wall (0 to 6), z is height (0 to 3.5). Sensor order matches the crate's
// idbd_lab::family::SENSORS.

import { svg } from "../src/dom.js";

export const SENSOR_POSITIONS = [
  [5.2, 0, 2.9], // Wi-Fi signal: the router high on the back wall
  [0.7, 0.7, 1.75], // water heater temperature, in the boiler room in the back corner
  [0, 4.9, 1.9], // doorbell battery, beside the front door
  [0, 1.9, 1.15], // wall temperature, on the hall wall
  [0, 2.65, 2.55], // barometer, hall wall
  [7.1, 0.8, 1.1], // oven temperature, in the kitchen corner
  [0, 2.65, 1.15], // humidity, hall wall
  [0, 1.9, 2.55], // CO2, hall wall
  [4.9, 3.0, 1.25], // Wi-Fi data use, on the TV on the middle block
  [2.1, 0, 2.6], // air quality, back wall by the window
  [0, 3.8, 1.25], // front door contact
  [3.9, 5.5, 0.55], // plant soil moisture, in the pot by the sofa
];

/** Sensors labelled in the 2D drawing, with label anchor offsets. */
const LABELS = new Map([
  [0, { dx: 40, dy: -34 }],
  [1, { dx: -30, dy: -72, end: true }],
  [2, { dx: -23, dy: 62, end: true }],
  [3, { dx: -119, dy: -8, end: true }],
  [4, { dx: -95, dy: -13, end: true }],
  [5, { dx: -10, dy: 60, end: true }],
  [6, { dx: -95, dy: 9, end: true }],
  [7, { dx: -109, dy: -31, end: true }],
  [8, { dx: 70, dy: 18 }],
  [9, { dx: 22, dy: -58 }],
  [10, { dx: -58, dy: 26, end: true }],
  [11, { dx: 50, dy: 30 }],
]);

/**
 * Each sensor's own color, used for its device in the house and its card. None of them is
 * IDBD's amber, the fixed learner's blue or the visitor's violet.
 */
export const SENSOR_COLORS = [
  "#22d3ee",
  "#cbd5e1",
  "#f472b6",
  "#fca5a5",
  "#a3e635",
  "#f87171",
  "#5eead4",
  "#86efac",
  "#fde68a",
  "#d6d3d1",
  "#34d399",
  "#bef264",
];

/** Each sensor's own blink period in seconds, so they never pulse in step. */
export const PERIODS = [0.9, 2.6, 2.9, 3.1, 3.7, 1.7, 2.3, 2.7, 1.3, 3.3, 2.1, 4.1];

/** The breakpoint below which the 2D drawing hides its labels, matching story.css. */
export const PHONE = "(max-width: 640px)";

/** Isometric projection into the 2D drawing's coordinates. */
export function project([x, y, z]) {
  return [300 + (x - y) * 32, 190 + (x + y) * 18.5 - z * 37];
}

/**
 * Glow radius in drawing units for a learning speed per unit of reading. The radius grows
 * with the square root, so a sensor with four times the speed glows twice as wide.
 */
export function glowRadius(speed) {
  return Math.max(5, Math.min(40, 120 * Math.sqrt(Math.max(speed, 0))));
}

/**
 * Ring opacity for each learning speed, relative to the fastest. The floor of 0.5 keeps
 * the faintest amber ring at 3:1 against the house's lightest surface (WCAG 1.4.11), so
 * every ring stays visible and its size carries the speed.
 */
export function ringOpacity(alphas) {
  const max = Math.max(...alphas);
  return alphas.map((a) => 0.5 + 0.45 * (max > 0 ? Math.max(0, a) / max : 0));
}

/**
 * Draws the sensors into the 2D house and returns an updater. Each device blinks in its
 * own color at its own period through CSS. IDBD's attention is an amber ring.
 */
export function createHouse2d(group, names) {
  const chips = [];
  const sensors = SENSOR_POSITIONS.map((position, index) => {
    const [cx, cy] = project(position);
    const ring = svg("circle", { cx, cy, r: 16, class: "attention" });
    const glow = svg("circle", { cx, cy, r: 9, class: "led-glow", fill: SENSOR_COLORS[index], filter: "url(#blur)" });
    const dot = svg("circle", { cx, cy, r: 4, class: "led", fill: SENSOR_COLORS[index] });
    glow.style.animationDuration = `${PERIODS[index]}s`;
    group.append(ring, glow, dot);
    const label = LABELS.get(index);
    if (label) {
      const x = cx + label.dx;
      const y = cy + label.dy;
      group.append(
        svg("line", { x1: cx, y1: cy, x2: x, y2: y + 4, class: "leader", stroke: "rgba(255,255,255,0.25)" }),
      );
      const text = svg("text", {
        x: label.end ? x - 4 : x + 12,
        y,
        class: "label",
        "text-anchor": label.end ? "end" : "start",
        fill: "var(--ink)",
      });
      text.textContent = names[index];
      group.append(text);
      // Same chip as the 3D labels: a dark pill with the sensor's dot.
      const chip = svg("rect", { rx: 8, class: "label-chip" });
      const chipDot = svg("circle", { r: 3, fill: SENSOR_COLORS[index] });
      group.insertBefore(chip, text);
      group.insertBefore(chipDot, text);
      chips.push({ text, chip, chipDot });
    }
    return { ring };
  });

  // A hidden label measures as an empty box, so the chips are sized whenever the labels
  // become visible: after the fonts load and whenever the phone breakpoint flips.
  function sizeChips() {
    for (const { text, chip, chipDot } of chips) {
      const box = text.getBBox();
      if (box.width === 0) continue;
      const left = box.x - 14;
      chip.setAttribute("x", left - 2);
      chip.setAttribute("y", box.y - 3);
      chip.setAttribute("width", box.width + 22);
      chip.setAttribute("height", box.height + 6);
      chipDot.setAttribute("cx", left + 6);
      chipDot.setAttribute("cy", box.y + box.height / 2);
    }
  }
  sizeChips();
  document.fonts?.ready.then(sizeChips);
  // WebKit fires the change before it restyles, so measure on the next frame.
  window.matchMedia(PHONE).addEventListener("change", () => requestAnimationFrame(sizeChips));

  return function update({ speeds }) {
    const opacity = ringOpacity(speeds);
    sensors.forEach(({ ring }, i) => {
      ring.setAttribute("r", glowRadius(speeds[i]).toFixed(1));
      ring.setAttribute("stroke-opacity", opacity[i].toFixed(2));
    });
  };
}
