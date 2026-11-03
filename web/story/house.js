// Where each sensor sits in the house, and the 2D drawing that shows the learned speeds.
// House coordinates: x runs along the right-hand back wall (0 to 8), y along the
// left-hand back wall (0 to 6), z is height (0 to 3.5). Sensor order matches the crate's
// idbd_lab::house::SENSORS.

export const SENSOR_POSITIONS = [
  [3, 0, 1.95], // sun on window
  [7.1, 0.8, 1.1], // oven
  [0, 3.8, 1.25], // front door
  [5.2, 0, 2.9], // wi-fi signal
  [0, 4.9, 1.9], // doorbell battery
  [0.7, 0.7, 1.75], // water heater, in the boiler room in the back corner
];

/** Sensors labelled in the 2D drawing, with label anchor offsets. */
const LABELS = new Map([
  [0, { dx: 44, dy: -80 }],
  [1, { dx: 82, dy: -22 }],
  [2, { dx: 80, dy: 40 }],
  [3, { dx: 30, dy: -40, muted: true }],
  [4, { dx: 10, dy: 100, muted: true }],
  [5, { dx: -50, dy: -30, muted: true, end: true }],
]);

/**
 * Each sensor's own color, used for its device in the house, its slider and its card.
 * None of them is IDBD's amber, the fixed learner's blue or the visitor's violet.
 */
export const SENSOR_COLORS = ["#fde68a", "#f87171", "#34d399", "#22d3ee", "#f472b6", "#cbd5e1"];

const SVG_NS = "http://www.w3.org/2000/svg";

/** Isometric projection into the 2D drawing's coordinates. */
export function project([x, y, z]) {
  return [300 + (x - y) * 32, 190 + (x + y) * 18.5 - z * 37];
}

/**
 * Glow radius in drawing units for a learned step size. The radius grows with the square
 * root, so a sensor with four times the speed glows twice as wide.
 */
export function glowRadius(alpha) {
  return Math.max(6, Math.min(40, 95 * Math.sqrt(Math.max(alpha, 0))));
}

/**
 * How strongly a sensor reads as trusted, from 0 to 1, relative to the fastest and the
 * median learned speed. Driven only by IDBD's step sizes.
 */
export function trust(alphas) {
  const sorted = [...alphas].sort((a, b) => a - b);
  const median = sorted[Math.floor(sorted.length / 2)];
  const max = sorted.at(-1);
  return alphas.map((a) => (max > median ? Math.min(1, Math.max(0, (a - median) / (max - median))) : 0));
}

function el(name, attributes = {}) {
  const node = document.createElementNS(SVG_NS, name);
  for (const [key, value] of Object.entries(attributes)) {
    node.setAttribute(key, String(value));
  }
  return node;
}

/**
 * Draws the sensors into the 2D house and returns an updater. Each device blinks in its
 * own color at its own period through CSS. IDBD's attention is an amber ring.
 */
export function createHouse2d(group, names) {
  const sensors = SENSOR_POSITIONS.map((position, index) => {
    const [cx, cy] = project(position);
    const ring = el("circle", { cx, cy, r: 16, class: "attention" });
    const glow = el("circle", { cx, cy, r: 9, class: "led-glow", fill: SENSOR_COLORS[index], filter: "url(#blur)" });
    const dot = el("circle", { cx, cy, r: 4, class: "led", fill: SENSOR_COLORS[index] });
    glow.style.animationDuration = `${[3.1, 1.7, 2.3, 0.9, 2.9, 1.0][index]}s`;
    group.append(ring, glow, dot);
    const label = LABELS.get(index);
    if (label) {
      const x = cx + label.dx;
      const y = cy + label.dy;
      group.append(
        el("line", { x1: cx, y1: cy, x2: x, y2: y + 4, class: "leader", stroke: "rgba(255,255,255,0.25)" }),
      );
      const text = el("text", {
        x: label.end ? x - 4 : x + 12,
        y,
        class: "label",
        "text-anchor": label.end ? "end" : "start",
        fill: "var(--ink)",
      });
      text.textContent = names[index];
      group.append(text);
      // Same chip as the 3D labels: a dark pill with the sensor's dot.
      const box = text.getBBox();
      const left = label.end ? box.x - 14 : box.x - 14;
      const chip = el("rect", {
        x: left - 2,
        y: box.y - 3,
        width: box.width + 22,
        height: box.height + 6,
        rx: 8,
        class: "label-chip",
      });
      const chipDot = el("circle", { cx: left + 6, cy: box.y + box.height / 2, r: 3, fill: SENSOR_COLORS[index] });
      group.insertBefore(chip, text);
      group.insertBefore(chipDot, text);
    }
    return { ring };
  });

  return function update({ alphas }) {
    const t = trust(alphas);
    sensors.forEach(({ ring }, i) => {
      ring.setAttribute("r", glowRadius(alphas[i]).toFixed(1));
      ring.setAttribute("stroke-opacity", (0.2 + 0.7 * t[i]).toFixed(2));
    });
  };
}
