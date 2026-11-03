// Hand-written SVG charts. Each one is redrawn at the container's pixel width, so
// lines stay one pixel wide and labels keep their type size at any viewport.

import { svg, clear, tooltip } from "./dom.js";
import { fmtAlpha, fmtCount, fmtMse, fmtSigned } from "./format.js";

const IDBD = "var(--idbd)";
const LMS = "var(--lms)";
const AUTOSTEP = "var(--autostep)";

/**
 * `measured` is the container width the caller read before its DOM writes. Reading
 * `clientWidth` after a write forces a synchronous layout.
 */
function frame(container, height, margin, measured) {
  const width = Math.max(240, Math.floor(measured ?? container.clientWidth) || 320);
  const plotWidth = Math.max(40, width - margin.left - margin.right);
  const plotHeight = Math.max(40, height - margin.top - margin.bottom);
  const root = svg("svg", {
    width,
    height,
    viewBox: `0 0 ${width} ${height}`,
    role: "img",
  });
  return { width, height, plotWidth, plotHeight, margin, root };
}

function niceTicks(min, max, count) {
  if (!(max > min)) {
    return [min];
  }
  const raw = (max - min) / Math.max(1, count);
  const magnitude = 10 ** Math.floor(Math.log10(raw));
  const normalized = raw / magnitude;
  const step =
    (normalized >= 5 ? 10 : normalized >= 2 ? 5 : normalized >= 1 ? 2 : 1) * magnitude;
  const first = Math.ceil(min / step) * step;
  const ticks = [];
  for (let value = first; value <= max + step * 1e-9; value += step) {
    ticks.push(Number(value.toFixed(12)));
  }
  return ticks;
}

function logLabel(value) {
  if (value >= 0.001 && value < 1000) {
    return String(Number(value.toPrecision(2)));
  }
  const power = Math.round(Math.log10(value));
  const mantissa = value / 10 ** power;
  return Math.abs(mantissa - 1) < 1e-9
    ? `1e${power}`
    : `${Number(mantissa.toPrecision(1))}e${power}`;
}

/**
 * Ticks for a base-10 log axis: decades, with 2 and 5 steps added when the range
 * covers fewer than three decades and bare decades would leave the axis unlabeled.
 */
function logTicks(min, max) {
  const low = Math.floor(Math.log10(min));
  const high = Math.ceil(Math.log10(max));
  const inRange = (value) => value >= min * 0.999 && value <= max * 1.001;
  const decades = [];
  for (let power = low; power <= high; power += 1) {
    if (inRange(10 ** power)) {
      decades.push(10 ** power);
    }
  }
  if (decades.length >= 3) {
    return decades.map((value) => [value, logLabel(value)]);
  }
  const ticks = [];
  for (let power = low; power <= high; power += 1) {
    for (const step of [1, 2, 5]) {
      const value = step * 10 ** power;
      if (inRange(value)) {
        ticks.push(value);
      }
    }
  }
  return ticks.map((value) => [value, logLabel(value)]);
}

/** A short axis label for a mean squared error. */
function mseTick(value) {
  if (value === 0) {
    return "0";
  }
  if (Math.abs(value) >= 1) {
    return value.toFixed(1);
  }
  return Math.abs(value) >= 0.01 ? value.toFixed(3) : value.toExponential(1);
}

/** The empty-state line for a time chart, which needs two samples to draw a line. */
function waiting(count) {
  return count === 0
    ? "No completed samples yet. Choose Step or Run."
    : "One sample so far. The line starts at sample 2.";
}

function emptyState(container, height, message, measured) {
  inspection.set(container, null);
  const { root, width } = frame(container, height, { top: 0, right: 0, bottom: 0, left: 0 }, measured);
  root.setAttribute("aria-label", message);
  root.append(
    svg("text", {
      x: 12,
      y: Math.round(height / 2),
      class: "empty-note",
      text: message,
    }),
  );
  clear(container);
  container.append(root);
  return width;
}

/**
 * Marks structural events on a time chart. Sign flips happen every 20 samples in the
 * paper task, so only relevance shifts are drawn here; the timeline carries the rest.
 */
function eventMarks(group, events, xOf, top, bottom) {
  for (const event of events) {
    if (event.kind !== "relevanceShift") {
      continue;
    }
    const x = xOf(event.sample);
    if (!Number.isFinite(x)) {
      continue;
    }
    group.append(svg("line", { x1: x, y1: top, x2: x, y2: bottom, class: "event-mark" }));
    group.append(
      svg("path", {
        d: `M${x - 4} ${top} L${x + 4} ${top} L${x} ${top + 7} Z`,
        class: "event-glyph",
      }),
    );
  }
}

function indexAt(x, geometry, count) {
  const ratio = (x - geometry.margin.left) / geometry.plotWidth;
  return Math.min(count - 1, Math.max(0, Math.round(ratio * (count - 1))));
}

/**
 * Keyboard reading for a time chart: the container takes focus, arrow keys move a
 * crosshair and a visible readout shows the same values the pointer tooltip does. The
 * position is kept as an offset from the newest sample, so it holds while a run adds
 * samples and the charts redraw.
 */
export const inspection = {
  charts: new WeakMap(),
  attach(container, readout) {
    const entry = { readout, chart: null, back: null };
    this.charts.set(container, entry);
    container.tabIndex = 0;
    container.addEventListener("keydown", (event) => {
      if (!entry.chart) {
        return;
      }
      const last = entry.chart.count - 1;
      const index = entry.back === null ? last : Math.max(0, last - entry.back);
      const step = event.shiftKey ? 10 : 1;
      const moves = {
        ArrowLeft: index - step,
        ArrowRight: index + step,
        Home: 0,
        End: last,
      };
      if (event.key in moves) {
        event.preventDefault();
        entry.back = last - Math.max(0, Math.min(last, moves[event.key]));
        this.show(entry);
      } else if (event.key === "Escape") {
        this.clear(entry);
      }
    });
    container.addEventListener("blur", () => this.clear(entry));
  },
  set(container, chart) {
    const entry = this.charts.get(container);
    if (!entry) {
      return;
    }
    entry.chart = chart;
    if (!chart) {
      this.clear(entry);
    } else if (entry.back !== null) {
      this.show(entry);
    }
  },
  show(entry) {
    const index = Math.max(0, entry.chart.count - 1 - entry.back);
    entry.chart.mark(index);
    entry.readout.textContent = entry.chart.lines(index).join(", ");
    entry.readout.hidden = false;
  },
  clear(entry) {
    entry.back = null;
    entry.chart?.mark(null);
    entry.readout.textContent = "";
    entry.readout.hidden = true;
  },
};

function hitArea(root, geometry, onMove) {
  const { margin, plotWidth, plotHeight } = geometry;
  const area = svg("rect", {
    x: margin.left,
    y: margin.top,
    width: plotWidth,
    height: plotHeight,
    class: "hit-area",
  });
  area.addEventListener("pointermove", (pointer) => {
    const box = root.getBoundingClientRect();
    onMove(pointer.clientX - box.left, pointer, box);
  });
  area.addEventListener("pointerleave", () => tooltip.hide());
  root.append(area);
  return area;
}

/**
 * Two rolling mean squared error series against the sample index, with world events
 * marked and a crosshair tooltip.
 */
export function renderErrorChart(container, data) {
  const { samples, idbd, lms, autostep, events, label, width: measured } = data;
  if (samples.length < 2) {
    emptyState(container, 240, waiting(samples.length), measured);
    return;
  }
  const geometry = frame(container, 240, { top: 18, right: 54, bottom: 30, left: 56 }, measured);
  const { root, margin, plotWidth, plotHeight } = geometry;
  root.setAttribute("aria-label", label);

  const xMin = samples[0];
  const xMax = samples[samples.length - 1];
  const finite = [...idbd, ...lms, ...autostep].filter((value) => Number.isFinite(value));
  const yMax = Math.max(1e-6, ...finite) * 1.12;
  const xOf = (sample) =>
    margin.left + ((sample - xMin) / Math.max(1, xMax - xMin)) * plotWidth;
  const yOf = (value) => margin.top + plotHeight * (1 - value / yMax);

  for (const tick of niceTicks(0, yMax, 4)) {
    const y = yOf(tick);
    root.append(
      svg("line", { x1: margin.left, y1: y, x2: margin.left + plotWidth, y2: y, class: "grid-line" }),
    );
    root.append(
      svg("text", {
        x: margin.left - 8,
        y: y + 3,
        class: "tick-label",
        "text-anchor": "end",
        text: mseTick(tick),
      }),
    );
  }
  for (const tick of niceTicks(xMin, xMax, 4)) {
    const x = xOf(tick);
    root.append(
      svg("text", {
        x,
        y: margin.top + plotHeight + 16,
        class: "tick-label",
        "text-anchor": "middle",
        text: fmtCount(Math.round(tick)),
      }),
    );
  }
  root.append(
    svg("line", {
      x1: margin.left,
      y1: margin.top + plotHeight,
      x2: margin.left + plotWidth,
      y2: margin.top + plotHeight,
      class: "axis-line",
    }),
  );
  root.append(
    svg("text", { x: 0, y: 10, class: "axis-title", text: "rolling MSE" }),
  );
  root.append(
    svg("text", {
      x: margin.left + plotWidth,
      y: margin.top + plotHeight + 26,
      class: "axis-title",
      "text-anchor": "end",
      text: "completed sample",
    }),
  );

  eventMarks(root, events, xOf, margin.top, margin.top + plotHeight);

  // A diverged learner's rolling MSE overflows to infinity. The line stops at its last
  // finite value and climbs to the top edge, so divergence never reads as zero error.
  const path = (values) => {
    let d = "";
    let drawing = false;
    values.forEach((value, index) => {
      const x = xOf(samples[index]).toFixed(2);
      if (Number.isFinite(value)) {
        d += `${drawing ? "L" : "M"}${x} ${yOf(value).toFixed(2)}`;
        drawing = true;
      } else if (drawing) {
        d += `L${x} ${margin.top}`;
        drawing = false;
      }
    });
    return d;
  };
  const overflow = (values, className) => {
    const index = values.findIndex((value) => !Number.isFinite(value));
    if (index < 0) {
      return;
    }
    const x = xOf(samples[index]);
    root.append(svg("path", { d: `M${x - 4} ${margin.top + 6} L${x} ${margin.top} L${x + 4} ${margin.top + 6}`, class: className }));
    root.append(
      svg("text", {
        x: Math.min(x + 6, margin.left + plotWidth - 4),
        y: margin.top + 10,
        class: "tick-label",
        text: "overflow",
      }),
    );
  };
  root.append(svg("path", { d: path(autostep), class: "series-autostep" }));
  root.append(svg("path", { d: path(lms), class: "series-lms" }));
  root.append(svg("path", { d: path(idbd), class: "series-idbd" }));
  overflow(autostep, "series-autostep");
  overflow(lms, "series-lms");
  overflow(idbd, "series-idbd");

  const last = samples.length - 1;
  const endY = (value) => (Number.isFinite(value) ? yOf(value) : margin.top) + 3;
  root.append(
    svg("text", {
      x: margin.left + plotWidth + 6,
      y: endY(idbd[last]),
      class: "tick-label",
      fill: IDBD,
      text: "IDBD",
    }),
  );
  root.append(
    svg("text", {
      x: margin.left + plotWidth + 6,
      y: endY(lms[last]),
      class: "tick-label",
      fill: LMS,
      text: "LMS",
    }),
  );
  root.append(
    svg("text", {
      x: margin.left + plotWidth + 6,
      y: endY(autostep[last]),
      class: "tick-label",
      fill: AUTOSTEP,
      text: "AUTO",
    }),
  );

  const crosshair = svg("line", {
    y1: margin.top,
    y2: margin.top + plotHeight,
    class: "crosshair",
    opacity: 0,
  });
  root.append(crosshair);
  const lines = (index) => [
    `sample ${fmtCount(samples[index])}`,
    `IDBD rolling MSE ${fmtMse(idbd[index])}`,
    `LMS rolling MSE ${fmtMse(lms[index])}`,
    `Autostep rolling MSE ${fmtMse(autostep[index])}`,
  ];
  const mark = (index) => {
    if (index === null) {
      crosshair.setAttribute("opacity", 0);
      return;
    }
    const cx = xOf(samples[index]);
    crosshair.setAttribute("x1", cx);
    crosshair.setAttribute("x2", cx);
    crosshair.setAttribute("opacity", 1);
  };
  hitArea(root, geometry, (x, pointer) => {
    const index = indexAt(x, geometry, samples.length);
    mark(index);
    tooltip.show(lines(index), pointer.clientX, pointer.clientY);
  });

  clear(container);
  container.append(root);
  inspection.set(container, { count: samples.length, lines, mark });
}

/** Every input's step size against the sample index, on a base-10 log scale. */
export function renderAlphaChart(container, data) {
  const { samples, series, relevant, selected, events, width: measured } = data;
  if (samples.length < 2) {
    emptyState(container, 352, waiting(samples.length), measured);
    return;
  }
  const geometry = frame(container, 352, { top: 18, right: 16, bottom: 30, left: 56 }, measured);
  const { root, margin, plotWidth, plotHeight } = geometry;
  root.setAttribute(
    "aria-label",
    "Step size alpha for each of the 20 inputs over the retained samples, on a base-10 logarithmic scale",
  );

  let low = Infinity;
  let high = -Infinity;
  for (const values of series) {
    for (const value of values) {
      if (Number.isFinite(value) && value > 0) {
        low = Math.min(low, value);
        high = Math.max(high, value);
      }
    }
  }
  if (!Number.isFinite(low) || !Number.isFinite(high)) {
    emptyState(container, 352, "No step sizes recorded yet.", measured);
    return;
  }
  // Padding on a log scale. A fixed floor would sit above a tiny valid alpha0 and invert
  // the axis.
  low /= 1.6;
  high *= 1.6;

  const xMin = samples[0];
  const xMax = samples[samples.length - 1];
  const xOf = (sample) =>
    margin.left + ((sample - xMin) / Math.max(1, xMax - xMin)) * plotWidth;
  const logLow = Math.log10(low);
  const logHigh = Math.log10(high);
  const yOf = (value) => {
    const clamped = Math.max(low, Math.min(high, value));
    return margin.top + plotHeight * (1 - (Math.log10(clamped) - logLow) / (logHigh - logLow));
  };

  for (const [tick, label] of logTicks(low, high)) {
    const y = yOf(tick);
    root.append(
      svg("line", { x1: margin.left, y1: y, x2: margin.left + plotWidth, y2: y, class: "grid-line" }),
    );
    root.append(
      svg("text", {
        x: margin.left - 8,
        y: y + 3,
        class: "tick-label",
        "text-anchor": "end",
        text: label,
      }),
    );
  }
  for (const tick of niceTicks(xMin, xMax, 4)) {
    root.append(
      svg("text", {
        x: xOf(tick),
        y: margin.top + plotHeight + 16,
        class: "tick-label",
        "text-anchor": "middle",
        text: fmtCount(Math.round(tick)),
      }),
    );
  }
  root.append(
    svg("line", {
      x1: margin.left,
      y1: margin.top + plotHeight,
      x2: margin.left + plotWidth,
      y2: margin.top + plotHeight,
      class: "axis-line",
    }),
  );
  root.append(
    svg("text", { x: 0, y: 10, class: "axis-title", text: "alpha, log scale" }),
  );

  eventMarks(root, events, xOf, margin.top, margin.top + plotHeight);

  const draw = (index) => {
    const values = series[index];
    const d = values
      .map((value, i) => `${i === 0 ? "M" : "L"}${xOf(samples[i]).toFixed(2)} ${yOf(value).toFixed(2)}`)
      .join(" ");
    const classes = ["alpha-line"];
    if (!relevant[index]) {
      classes.push("irrelevant");
    }
    if (index === selected) {
      classes.push("selected");
    }
    root.append(svg("path", { d, class: classes.join(" ") }));
  };
  for (let index = 0; index < series.length; index += 1) {
    if (index !== selected) {
      draw(index);
    }
  }
  if (selected >= 0 && selected < series.length) {
    draw(selected);
  }

  const crosshair = svg("line", {
    y1: margin.top,
    y2: margin.top + plotHeight,
    class: "crosshair",
    opacity: 0,
  });
  root.append(crosshair);
  const lines = (index) => {
    const rows = [`sample ${fmtCount(samples[index])}`];
    if (selected >= 0) {
      rows.push(`x${selected} alpha ${fmtAlpha(series[selected][index])}`);
    }
    const relevantValues = series.filter((_, i) => relevant[i]).map((values) => values[index]);
    if (relevantValues.length) {
      rows.push(`relevant inputs ${fmtAlpha(Math.min(...relevantValues))} to ${fmtAlpha(Math.max(...relevantValues))}`);
    }
    return rows;
  };
  const mark = (index) => {
    if (index === null) {
      crosshair.setAttribute("opacity", 0);
      return;
    }
    const cx = xOf(samples[index]);
    crosshair.setAttribute("x1", cx);
    crosshair.setAttribute("x2", cx);
    crosshair.setAttribute("opacity", 1);
  };
  hitArea(root, geometry, (x, pointer) => {
    const index = indexAt(x, geometry, samples.length);
    mark(index);
    tooltip.show(lines(index), pointer.clientX, pointer.clientY);
  });

  clear(container);
  container.append(root);
  inspection.set(container, { count: samples.length, lines, mark });
}

const TIMELINE_HEIGHT = 44;

/**
 * A strip of world events over the retained sample window. Sign flips are ticks and
 * relevance shifts are labeled flags, so a dense run of flips stays readable.
 */
export function renderTimeline(container, data) {
  const { samples, events, width: measured } = data;
  if (samples.length < 2) {
    emptyState(container, TIMELINE_HEIGHT, waiting(samples.length), measured);
    return;
  }
  const geometry = frame(container, TIMELINE_HEIGHT, { top: 6, right: 16, bottom: 20, left: 16 }, measured);
  const { root, margin, plotWidth, plotHeight } = geometry;
  root.setAttribute("aria-label", "World events over the retained samples");

  const xMin = samples[0];
  const xMax = samples[samples.length - 1];
  const xOf = (sample) =>
    margin.left + ((sample - xMin) / Math.max(1, xMax - xMin)) * plotWidth;
  const baseline = margin.top + plotHeight;
  root.append(
    svg("line", {
      x1: margin.left,
      y1: baseline,
      x2: margin.left + plotWidth,
      y2: baseline,
      class: "axis-line",
    }),
  );
  for (const tick of niceTicks(xMin, xMax, 4)) {
    root.append(
      svg("text", {
        x: xOf(tick),
        y: baseline + 14,
        class: "tick-label",
        "text-anchor": "middle",
        text: fmtCount(Math.round(tick)),
      }),
    );
  }
  for (const event of events) {
    const x = xOf(event.sample);
    if (!Number.isFinite(x) || x < margin.left - 1 || x > margin.left + plotWidth + 1) {
      continue;
    }
    if (event.kind === "relevanceShift") {
      root.append(
        svg("line", { x1: x, y1: margin.top, x2: x, y2: baseline, class: "event-mark" }),
      );
      root.append(
        svg("path", {
          d: `M${x - 4} ${margin.top} L${x + 4} ${margin.top} L${x} ${margin.top + 7} Z`,
          class: "event-glyph",
        }),
      );
    } else {
      root.append(
        svg("line", { x1: x, y1: margin.top + 4, x2: x, y2: baseline, class: "flip-mark" }),
      );
    }
  }
  clear(container);
  container.append(root);
}

/** The selected input's step size history, log scale, as a small labeled plot. */
export function renderSparkline(container, data) {
  const { samples, values, name, width: measured } = data;
  if (samples.length < 2) {
    emptyState(container, 90, waiting(samples.length), measured);
    return;
  }
  const geometry = frame(container, 90, { top: 10, right: 12, bottom: 18, left: 50 }, measured);
  const { root, margin, plotWidth, plotHeight } = geometry;
  root.setAttribute("aria-label", `Step size alpha of input ${name}, base-10 logarithmic scale`);

  const positive = values.filter((value) => Number.isFinite(value) && value > 0);
  if (!positive.length) {
    emptyState(container, 90, "No step-size history yet.", measured);
    return;
  }
  const low = Math.min(...positive) / 1.6;
  const high = Math.max(...positive) * 1.6;
  const xOf = (index) => margin.left + (index / Math.max(1, samples.length - 1)) * plotWidth;
  const logLow = Math.log10(low);
  const logHigh = Math.log10(high);
  const yOf = (value) =>
    margin.top +
    plotHeight *
      (1 - (Math.log10(Math.max(low, Math.min(high, value))) - logLow) / (logHigh - logLow));

  for (const [tick, label] of logTicks(low, high)) {
    const y = yOf(tick);
    root.append(
      svg("line", { x1: margin.left, y1: y, x2: margin.left + plotWidth, y2: y, class: "grid-line" }),
    );
    root.append(
      svg("text", {
        x: margin.left - 6,
        y: y + 3,
        class: "tick-label",
        "text-anchor": "end",
        text: label,
      }),
    );
  }
  root.append(
    svg("path", {
      d: values
        .map((value, index) => `${index === 0 ? "M" : "L"}${xOf(index).toFixed(2)} ${yOf(value).toFixed(2)}`)
        .join(" "),
      class: "series-idbd",
    }),
  );
  root.append(
    svg("text", {
      x: margin.left,
      y: margin.top + plotHeight + 14,
      class: "tick-label",
      text: `samples ${fmtCount(samples[0])} to ${fmtCount(samples[samples.length - 1])}`,
    }),
  );
  clear(container);
  container.append(root);
}

/**
 * One input's weights: a coral IDBD bar and a blue LMS bar inside a neutral outline
 * that marks the true coefficient. Zero sits in the middle.
 */
export function renderWeightRow(container, data) {
  const { idbd, lms, truth, scale, width: measured } = data;
  const width = Math.max(120, Math.floor(measured ?? container.clientWidth) || 160);
  const height = 20;
  const root = svg("svg", {
    width,
    height,
    viewBox: `0 0 ${width} ${height}`,
    "aria-hidden": "true",
    focusable: "false",
  });
  const mid = Math.round(width / 2);
  const half = mid - 2;
  const xOf = (value) => mid + (Math.max(-scale, Math.min(scale, value)) / scale) * half;

  root.append(
    svg("line", { x1: mid, y1: 0, x2: mid, y2: height, class: "grid-line" }),
  );

  const truthX = xOf(truth);
  if (Math.abs(truthX - mid) > 0.5) {
    root.append(
      svg("rect", {
        x: Math.min(mid, truthX),
        y: 1.5,
        width: Math.abs(truthX - mid),
        height: height - 3,
        fill: "none",
        stroke: "var(--truth)",
        "stroke-width": 1,
        rx: 2,
      }),
    );
  } else {
    root.append(
      svg("line", { x1: mid, y1: 1.5, x2: mid, y2: height - 1.5, stroke: "var(--truth)", "stroke-width": 1 }),
    );
  }

  const bar = (value, y, color) => {
    const x = xOf(value);
    const left = Math.min(mid, x);
    const span = Math.max(1, Math.abs(x - mid));
    root.append(
      svg("rect", { x: left, y, width: span, height: 7, fill: color, rx: 2 }),
    );
  };
  bar(idbd, 1.5, IDBD);
  bar(lms, 11.5, LMS);

  clear(container);
  container.append(root);
}

/** Formats one weight row's accessible name, so the bars are never the only reading. */
export function weightRowLabel(name, values) {
  return (
    `${name}: IDBD weight ${fmtSigned(values.idbd)}, ` +
    `LMS weight ${fmtSigned(values.lms)}, ` +
    `true coefficient ${fmtSigned(values.truth, 3)}, ` +
    `alpha ${fmtAlpha(values.alpha)}`
  );
}
