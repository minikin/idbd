// Number formatting shared by the panels, the tables and the browser tests.
// Pure functions, no DOM: the Playwright specs import this file so that a test
// compares a table cell against the same formatting the page applied.

const MINUS = "-";

// Shown instead of a number before the first completed sample. A missing measurement is
// never displayed as zero.
export const NO_SAMPLE = "no sample yet";

/** Formats a number with a fixed number of decimals. */
export function fmt(value, digits = 4) {
  if (value === null || value === undefined) {
    return NO_SAMPLE;
  }
  if (!Number.isFinite(value)) {
    return "not finite";
  }
  return value.toFixed(digits);
}

/** Formats a number with an explicit sign, so a direction is never ambiguous. */
export function fmtSigned(value, digits = 4) {
  if (value === null || value === undefined) {
    return NO_SAMPLE;
  }
  if (!Number.isFinite(value)) {
    return "not finite";
  }
  const body = Math.abs(value).toFixed(digits);
  return `${value < 0 ? MINUS : "+"}${body}`;
}

/** Formats a step size, which spans several orders of magnitude. */
export function fmtAlpha(value) {
  if (value === null || value === undefined) {
    return NO_SAMPLE;
  }
  if (!Number.isFinite(value)) {
    return "not finite";
  }
  if (value === 0) {
    return "0";
  }
  return Math.abs(value) >= 1e-3 ? value.toFixed(6) : value.toExponential(3);
}

/**
 * Formats a setting the visitor typed, such as theta or a step size, with up to six
 * significant digits. Fixed decimals would print a valid 0.000001 as 0.0000.
 */
export function fmtSetting(value) {
  if (!Number.isFinite(value)) {
    return "not finite";
  }
  return String(Number(value.toPrecision(6)));
}

/** Formats a mean squared error. */
export function fmtMse(value) {
  if (value === null || value === undefined) {
    return NO_SAMPLE;
  }
  if (!Number.isFinite(value)) {
    return "not finite";
  }
  if (value === 0) {
    return "0";
  }
  return Math.abs(value) >= 1e-3 ? value.toFixed(4) : value.toExponential(3);
}

/** Formats a sample index with thousands separators. */
export function fmtCount(value) {
  return Number(value).toLocaleString("en-US");
}

/** The word for the sign of a value, for notes that explain a direction. */
export function signWord(value) {
  if (value > 0) {
    return "positive";
  }
  if (value < 0) {
    return "negative";
  }
  return "zero";
}

/** The name the whole demo uses for input `index`. */
export function featureName(index) {
  return `x${index}`;
}
