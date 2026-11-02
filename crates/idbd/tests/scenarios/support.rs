//! Shared helpers for the scenario tests.

use idbd::{Bounds, Config, Idbd, State};

/// The equation tolerance from the BDD specification.
pub(crate) fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-12 + 1e-10 * a.abs().max(b.abs())
}

#[track_caller]
pub(crate) fn assert_close(actual: f64, expected: f64, what: &str) {
    assert!(
        close(actual, expected),
        "{what}: got {actual:e}, expected {expected:e}"
    );
}

#[track_caller]
pub(crate) fn assert_all_close(actual: &[f64], expected: &[f64], what: &str) {
    assert_eq!(actual.len(), expected.len(), "{what}: length");
    for (i, (a, e)) in actual.iter().zip(expected).enumerate() {
        assert_close(*a, *e, &format!("{what}[{i}]"));
    }
}

fn bits(values: &[f64]) -> Vec<u64> {
    values.iter().map(|v| v.to_bits()).collect()
}

/// Asserts bitwise equality of configuration, weights, betas and traces.
#[track_caller]
pub(crate) fn assert_same_state(actual: &State, expected: &State) {
    assert_eq!(actual.config, expected.config, "configuration");
    assert_eq!(bits(&actual.weights), bits(&expected.weights), "weights");
    assert_eq!(bits(&actual.betas), bits(&expected.betas), "betas");
    assert_eq!(bits(&actual.traces), bits(&expected.traces), "traces");
}

/// 2 features, alpha 0.1, theta 0.01, no bounds.
pub(crate) fn standard_learner() -> Idbd {
    Idbd::new(2, config(0.1, 0.01)).expect("standard learner")
}

pub(crate) fn config(initial_step_size: f64, meta_step_size: f64) -> Config {
    Config::new(initial_step_size, meta_step_size).expect("valid configuration")
}

pub(crate) fn safety_config(initial_step_size: f64, meta_step_size: f64) -> Config {
    config(initial_step_size, meta_step_size)
        .with_bounds(Bounds::sutton_safety())
        .expect("initial beta within the safety bounds")
}

pub(crate) fn restore(config: Config, weights: &[f64], betas: &[f64], traces: &[f64]) -> Idbd {
    Idbd::from_state(State {
        config,
        weights: weights.to_vec(),
        betas: betas.to_vec(),
        traces: traces.to_vec(),
    })
    .expect("valid restored state")
}

/// `SplitMix64`, used only to generate deterministic test inputs.
pub(crate) struct TestRng(u64);

impl TestRng {
    pub(crate) fn new(seed: u64) -> Self {
        Self(seed)
    }

    pub(crate) fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in [-scale, scale).
    pub(crate) fn symmetric(&mut self, scale: f64) -> f64 {
        let unit = f64::from(u32::try_from(self.next_u64() >> 40).expect("24 bits fit"))
            / f64::from(1_u32 << 24);
        (2.0 * unit - 1.0) * scale
    }
}
