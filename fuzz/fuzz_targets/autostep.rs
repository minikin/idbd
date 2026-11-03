//! Autostep with any configuration and any stream of samples: an accepted update leaves
//! a state that `Autostep::from_state` restores exactly, `predict` matches the reported
//! prediction, and a rejected update changes nothing. Any checkpoint is either rejected
//! or restored exactly.

#![no_main]

use arbitrary::Arbitrary;
use idbd::autostep::{Autostep, Config, State};
use libfuzzer_sys::fuzz_target;

#[derive(Debug, Arbitrary)]
struct Input {
    initial_step_size: f64,
    meta_step_size: f64,
    time_scale: f64,
    num_features: u8,
    /// Consecutive groups of `num_features + 1` values: the features, then the target.
    values: Vec<f64>,
    /// An arbitrary checkpoint to restore.
    checkpoint: (Vec<f64>, Vec<f64>, Vec<f64>, Vec<f64>),
}

fuzz_target!(|input: Input| {
    let Ok(config) = Config::new(input.initial_step_size)
        .and_then(|c| c.with_meta_step_size(input.meta_step_size))
        .and_then(|c| c.with_time_scale(input.time_scale))
    else {
        return;
    };

    let (weights, step_sizes, traces, normalizers) = input.checkpoint;
    let state = State::new(config, weights, step_sizes, traces, normalizers);
    if let Ok(restored) = Autostep::from_state(state.clone()) {
        assert_eq!(
            restored.snapshot(),
            state,
            "a restored learner keeps its checkpoint"
        );
    }

    let n = usize::from(input.num_features % 16) + 1;
    let mut learner = Autostep::new(n, config).expect("n is nonzero and the config is valid");
    for sample in input.values.chunks_exact(n + 1).take(256) {
        let (features, target) = (&sample[..n], sample[n]);
        let before = learner.snapshot();
        let predicted = learner.predict(features);
        match learner.update(features, target) {
            Ok(update) => {
                assert_eq!(
                    predicted.ok(),
                    Some(update.prediction),
                    "predict agrees with update"
                );
                let state = learner.snapshot();
                let restored = Autostep::from_state(state.clone())
                    .expect("an accepted update leaves a valid state");
                assert_eq!(restored.snapshot(), state);
            }
            Err(_) => assert_eq!(
                learner.snapshot(),
                before,
                "a rejected update changes nothing"
            ),
        }
    }
});
