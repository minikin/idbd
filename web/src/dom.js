// Small DOM and SVG helpers. No framework, no bundler.

const SVG_NS = "http://www.w3.org/2000/svg";

export function el(tag, attributes = {}, children = []) {
  const node = document.createElement(tag);
  apply(node, attributes);
  append(node, children);
  return node;
}

export function svg(tag, attributes = {}, children = []) {
  const node = document.createElementNS(SVG_NS, tag);
  apply(node, attributes);
  append(node, children);
  return node;
}

function apply(node, attributes) {
  for (const [name, value] of Object.entries(attributes)) {
    if (value === null || value === undefined || value === false) {
      continue;
    }
    if (name === "text") {
      node.textContent = String(value);
    } else if (name === "class") {
      node.setAttribute("class", value);
    } else {
      node.setAttribute(name, String(value));
    }
  }
}

function append(node, children) {
  const list = Array.isArray(children) ? children : [children];
  for (const child of list) {
    if (child === null || child === undefined) {
      continue;
    }
    node.append(typeof child === "string" ? document.createTextNode(child) : child);
  }
}

export function clear(node) {
  node.replaceChildren();
}

/**
 * Builds a definition list of label and value pairs into `node`. A third entry in a
 * pair is a class for that pair, such as "wide-pair" for a long note.
 */
export function fillPairs(node, pairs) {
  clear(node);
  for (const [label, value, className] of pairs) {
    node.append(
      el("div", { class: className }, [el("dt", { text: label }), el("dd", { text: value })]),
    );
  }
}

/** Builds a legend where every entry carries a label and a distinct shape. */
export function fillLegend(node, entries) {
  clear(node);
  for (const entry of entries) {
    node.append(
      el("span", {}, [
        legendShape(entry),
        el("span", { text: entry.label, class: "legend-label" }),
      ]),
    );
  }
}

function legendShape(entry) {
  const box = svg("svg", {
    class: "swatch",
    viewBox: "0 0 14 14",
    "aria-hidden": "true",
    focusable: "false",
  });
  if (entry.shape === "outline") {
    box.append(
      svg("rect", {
        x: 1.5,
        y: 2.5,
        width: 11,
        height: 9,
        fill: "none",
        stroke: entry.color,
        "stroke-width": 1.5,
      }),
    );
  } else if (entry.shape === "dashed") {
    box.append(
      svg("line", {
        x1: 0,
        y1: 7,
        x2: 14,
        y2: 7,
        stroke: entry.color,
        "stroke-width": 2,
        "stroke-dasharray": "2 3",
      }),
    );
  } else if (entry.shape === "line") {
    box.append(
      svg("line", { x1: 0, y1: 7, x2: 14, y2: 7, stroke: entry.color, "stroke-width": 2 }),
    );
  } else if (entry.shape === "diamond") {
    box.append(
      svg("path", {
        d: "M7 1.5 L12.5 7 L7 12.5 L1.5 7 Z",
        fill: "none",
        stroke: entry.color,
        "stroke-width": 1.5,
      }),
    );
  } else {
    box.append(svg("rect", { x: 0, y: 3, width: 14, height: 8, fill: entry.color, rx: 2 }));
  }
  return box;
}

/** A single floating tooltip, shared by every chart. */
export const tooltip = {
  node: null,
  attach() {
    this.node = document.getElementById("tooltip");
  },
  /** `lines` is a list of plain strings, one per row. */
  show(lines, x, y) {
    if (!this.node) {
      return;
    }
    this.node.replaceChildren(...lines.map((line) => el("div", { text: line })));
    this.node.hidden = false;
    const box = this.node.getBoundingClientRect();
    const left = Math.min(Math.max(8, x + 14), window.innerWidth - box.width - 8);
    const top = Math.min(Math.max(8, y - box.height - 12), window.innerHeight - box.height - 8);
    this.node.style.left = `${left}px`;
    this.node.style.top = `${top}px`;
  },
  hide() {
    if (this.node) {
      this.node.hidden = true;
    }
  },
};

/** Runs `render` now and again whenever `node` changes width. */
export function onResize(node, render) {
  let width = node.clientWidth;
  const observer = new ResizeObserver(() => {
    if (Math.abs(node.clientWidth - width) > 1) {
      width = node.clientWidth;
      render();
    }
  });
  observer.observe(node);
}
