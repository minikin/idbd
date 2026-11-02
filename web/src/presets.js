// Run copy and the demo's default settings.

/**
 * The LMS baseline's step size, in one place so it is easy to change. 0.03 is the
 * best value in the recorded Sutton 1992 Experiment 1 grid: 20 seeds, mean squared
 * error 3.40 with a standard error of 0.02. See RESULTS_URL.
 */
export const DEFAULT_LMS_STEP_SIZE = 0.03;

/** Where the step size above was selected. */
export const RESULTS_URL =
  "https://github.com/minikin/idbd/blob/main/docs/results/sutton1992-experiment1/README.md";

/** The IDBD initial step size, as in Sutton (1992). */
export const DEFAULT_INITIAL_STEP_SIZE = 0.05;

/**
 * The IDBD meta step size. The paper's Experiment 2 uses 0.001 over 250,000 samples;
 * this demo uses 0.01 so that step sizes visibly separate within a run a visitor will
 * actually sit through.
 */
export const DEFAULT_THETA = 0.01;

export const DEFAULT_SEED = 1;

export const THETA_NOTE =
  "0.01, adapted from the paper's Experiment 2 value of 0.001 so that adaptation is " +
  "visible within tens of thousands of samples.";

export const LMS_NOTE =
  "Fixed step size, shared by all 20 inputs. 0.03 is the best value in our Sutton 1992 " +
  "Experiment 1 grid, over 20 seeds, with a mean squared error of 3.40 and a standard " +
  "error of 0.02. That grid used the paper task, so it does not carry over to the other " +
  "tasks here.";

/** Playback speeds, in samples per second. */
export const SPEEDS = [
  { label: "1 sample / second", value: 1 },
  { label: "10 samples / second", value: 10 },
  { label: "100 samples / second", value: 100 },
  { label: "1,000 samples / second", value: 1000 },
  { label: "5,000 samples / second", value: 5000 },
  { label: "20,000 samples / second", value: 20000 },
];

export const DEFAULT_SPEED = 100;

/** Display copy per preset id. The ids come from the Rust core. */
export const PRESET_COPY = {
  sutton1992: {
    name: "Sutton 1992 (paper task)",
    description:
      "Experiment 1 of the paper. Twenty standard normal inputs, of which the same " +
      "five stay relevant throughout. Every 20 samples one of those five coefficients " +
      "is selected at random and switched in sign. The other fifteen inputs are " +
      "irrelevant and stay at zero.",
  },
  stationary: {
    name: "Stationary",
    description:
      "Five fixed relevant coefficients and fifteen irrelevant inputs. Nothing about " +
      "the target-generating relationship changes. Not a paper task: it is the control " +
      "case for the other three.",
  },
  "changing-relevance": {
    name: "Changing relevance (extension)",
    description:
      "A new stress test, not a paper reproduction. Sutton's sign flips continue, and " +
      "every 5,000 samples relevance moves to the next group of five inputs: the old " +
      "group drops to zero and the new group gets fresh signs.",
  },
  drifting: {
    name: "Drifting (extension)",
    description:
      "A new stress test, not a paper reproduction. The five relevant coefficients " +
      "follow independent AR(1) random walks with autocorrelation 0.999, so the " +
      "relationship moves continuously instead of jumping.",
  },
};

/** Reference fixtures C06 and C07 from the crate's acceptance scenarios. */
export const REFERENCE_FIXTURE = {
  config: { numFeatures: 2, initialStepSize: 0.1, theta: 0.01 },
  samples: [
    { features: [1.0, 2.0], target: 1.0 },
    { features: [2.0, -1.0], target: 0.5 },
  ],
  expected: {
    prediction: 0,
    error: 0.5,
    weights: [0.20010005001667086, 0.15004997500833125],
    traces: [0.16006003001000252, 0.13006996501166376],
    betas: [Math.log(0.1) + 0.001, Math.log(0.1) - 0.001],
  },
};

/** The acceptance tolerance from the feature file. */
export function withinTolerance(a, b) {
  return Math.abs(a - b) <= 1e-12 + 1e-10 * Math.max(Math.abs(a), Math.abs(b));
}
