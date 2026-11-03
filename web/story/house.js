// Where each sensor sits in the house, and the 2D drawing that shows the learned speeds.
// House coordinates: x runs along the right-hand back wall (0 to 8), y along the
// left-hand back wall (0 to 6), z is height (0 to 3.5). Sensor order matches the crate's
// idbd_lab::house::SENSORS.

export const SENSOR_POSITIONS = [
  [3, 0, 1.95], // sun on window
  [7.1, 0.8, 1.1], // oven
  [0, 3.8, 1.25], // front door
  [5.2, 0, 2.9], // wi-fi signal
  [5.6, 0, 1.0], // router traffic
  [0, 4.9, 1.9], // doorbell battery
  [4.9, 3.0, 0.9], // phone charge
  [6.5, 5.4, 0.3], // plant soil
  [0, 5.4, 1.4], // tv standby
  [0, 2.2, 2.9], // clock
  [0, 1.0, 1.0], // mailbox
  [7.2, 0, 2.9], // printer ink
];

/** Sensors labelled in the 2D drawing, with label anchor offsets. */
const LABELS = new Map([
  [0, { dx: 44, dy: -80 }],
  [1, { dx: 82, dy: -22 }],
  [2, { dx: -108, dy: -64, end: true }],
  [3, { dx: 12, dy: -12, muted: true }],
  [8, { dx: -40, dy: 36, muted: true, end: true }],
  [6, { dx: 12, dy: 18, muted: true }],
]);

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

/** Trust above which a sensor is drawn in IDBD's color. */
export const TRUSTED = 0.3;

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

/** Draws the sensors into the 2D house and returns an updater for the glows. */
export function createHouse2d(group, names) {
  const sensors = SENSOR_POSITIONS.map((position, index) => {
    const [cx, cy] = project(position);
    const glow = el("circle", { cx, cy, r: 16, class: "glow", filter: "url(#blur)" });
    const dot = el("circle", { cx, cy, r: 3 });
    group.append(glow, dot);
    const label = LABELS.get(index);
    if (label) {
      const x = cx + label.dx;
      const y = cy + label.dy;
      group.append(
        el("line", { x1: cx, y1: cy, x2: x, y2: y + 4, class: "leader", stroke: "rgba(255,255,255,0.25)" }),
      );
      const text = el("text", {
        x: label.end ? x - 4 : x + 4,
        y,
        class: "label",
        "text-anchor": label.end ? "end" : "start",
        fill: label.muted ? "var(--ink-3)" : "var(--ink)",
      });
      text.textContent = names[index];
      group.append(text);
    }
    return { glow, dot };
  });

  return function update(alphas) {
    const t = trust(alphas);
    sensors.forEach(({ glow, dot }, i) => {
      glow.setAttribute("r", glowRadius(alphas[i]).toFixed(1));
      glow.setAttribute("fill", t[i] > TRUSTED ? "var(--idbd)" : "var(--ignored)");
      glow.setAttribute("opacity", (t[i] > TRUSTED ? 0.35 + 0.25 * t[i] : 0.14).toFixed(2));
      dot.setAttribute("fill", t[i] > TRUSTED ? "var(--idbd)" : "var(--ignored)");
    });
  };
}
