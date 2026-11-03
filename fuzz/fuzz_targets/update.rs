//! Any configuration and any stream of samples: an accepted update leaves a state that
//! `Idbd::from_state` accepts, and a rejected update changes nothing.

#![no_main]

use arbitrary::Arbitrary;
use idbd::{Bounds, Config, Idbd};
use libfuzzer_sys::fuzz_target;

#[derive(Debug, Arbitrary)]
struct Input {
    initial_step_size: f64,
    meta_step_size: f64,
    min_beta: Option<f64>,
    max_beta: Option<f64>,
    max_beta_increment: Option<f64>,
    num_features: u8,
    /// Consecutive groups of `num_features + 1` values: the features, then the target.
    values: Vec<f64>,
    /// Indices into the stream where the sample length is off by one.
    mismatches: Vec<u8>,
}

fn config(input: &Input) -> Option<Config> {
    let mut bounds = Bounds::none();
    if let Some(value) = input.min_beta {
        bounds = bounds.with_min_beta(value).ok()?;
    }
    if let Some(value) = input.max_beta {
        bounds = bounds.with_max_beta(value).ok()?;
    }
    if let Some(value) = input.max_beta_increment {
        bounds = bounds.with_max_beta_increment(value).ok()?;
    }
    Config::new(input.initial_step_size, input.meta_step_size)
        .ok()?
        .with_bounds(bounds)
        .ok()
}

fuzz_target!(|input: Input| {
    let Some(config) = config(&input) else {
        return;
    };
    let n = usize::from(input.num_features % 16) + 1;
    let mut learner = Idbd::new(n, config).expect("n is nonzero and the config is valid");
    for (index, sample) in input.values.chunks_exact(n + 1).take(256).enumerate() {
        let (mut features, target) = (&sample[..n], sample[n]);
        if input.mismatches.iter().any(|&m| usize::from(m) == index) {
            features = &features[..n - 1];
        }
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
                let restored = Idbd::from_state(state.clone())
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
