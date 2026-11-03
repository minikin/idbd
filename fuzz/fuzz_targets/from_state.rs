//! Any checkpoint: `Idbd::from_state` either rejects it or restores it exactly, and the
//! restored learner handles any sample without panicking.

#![no_main]

use arbitrary::Arbitrary;
use idbd::{Config, Idbd, State};
use libfuzzer_sys::fuzz_target;

#[derive(Debug, Arbitrary)]
struct Input {
    initial_step_size: f64,
    meta_step_size: f64,
    weights: Vec<f64>,
    betas: Vec<f64>,
    traces: Vec<f64>,
    features: Vec<f64>,
    target: f64,
}

fuzz_target!(|input: Input| {
    let Ok(config) = Config::new(input.initial_step_size, input.meta_step_size) else {
        return;
    };
    let state = State::new(config, input.weights, input.betas, input.traces);
    let Ok(mut learner) = Idbd::from_state(state.clone()) else {
        return;
    };
    assert_eq!(
        learner.snapshot(),
        state,
        "a restored learner keeps its checkpoint"
    );
    let before = learner.snapshot();
    if learner.update(&input.features, input.target).is_err() {
        assert_eq!(
            learner.snapshot(),
            before,
            "a rejected update changes nothing"
        );
    }
});
