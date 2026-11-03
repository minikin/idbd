//! Property tests that complement the scenario examples.

use idbd::{Bounds, Config, Error, Idbd};
use proptest::prelude::*;

use crate::support::{assert_all_close, assert_close, assert_same_state};

/// A plain transcription of Sutton (1992), equations 3 to 6, kept separate from the
/// crate so the two implementations can be compared.
struct Naive {
    theta: f64,
    bounds: Bounds,
    w: Vec<f64>,
    beta: Vec<f64>,
    h: Vec<f64>,
}

impl Naive {
    fn new(n: usize, config: &Config) -> Self {
        Self {
            theta: config.meta_step_size(),
            bounds: *config.bounds(),
            w: vec![0.0; n],
            beta: vec![config.initial_beta(); n],
            h: vec![0.0; n],
        }
    }

    #[expect(
        clippy::needless_range_loop,
        reason = "mirrors the per-index loop of the paper's pseudocode"
    )]
    fn update(&mut self, x: &[f64], target: f64) -> (f64, f64) {
        let y: f64 = self.w.iter().zip(x).map(|(w, x)| w * x).sum();
        let delta = target - y;
        for i in 0..x.len() {
            let mut increment = self.theta * delta * x[i] * self.h[i];
            if let Some(limit) = self.bounds.max_beta_increment() {
                increment = increment.clamp(-limit, limit);
            }
            let mut beta = self.beta[i] + increment;
            if let Some(min) = self.bounds.min_beta() {
                beta = beta.max(min);
            }
            if let Some(max) = self.bounds.max_beta() {
                beta = beta.min(max);
            }
            self.beta[i] = beta;
            let alpha = beta.exp();
            self.w[i] += alpha * delta * x[i];
            self.h[i] = self.h[i] * (1.0 - alpha * x[i] * x[i]).max(0.0) + alpha * delta * x[i];
        }
        (y, delta)
    }
}

fn config() -> impl Strategy<Value = Config> {
    (0.01..0.1_f64, 0.0..0.05_f64, 0..3_u8).prop_map(|(alpha, theta, kind)| {
        let bounds = match kind {
            0 => Bounds::none(),
            1 => Bounds::sutton_safety(),
            _ => Bounds::none()
                .with_min_beta(alpha.ln() - 0.5)
                .and_then(|b| b.with_max_beta(alpha.ln() + 0.2))
                .and_then(|b| b.with_max_beta_increment(0.01))
                .expect("valid generated bounds"),
        };
        Config::new(alpha, theta)
            .and_then(|c| c.with_bounds(bounds))
            .expect("valid generated config")
    })
}

/// A feature value; about one in five is exactly zero.
fn feature() -> impl Strategy<Value = f64> {
    prop_oneof![1 => Just(0.0), 4 => -1.5..1.5_f64]
}

/// A learner size, a configuration and a stream of examples for that size.
fn scenario(steps: usize) -> impl Strategy<Value = (Config, Vec<(Vec<f64>, f64)>)> {
    (1..=6_usize, config()).prop_flat_map(move |(n, config)| {
        let example = (prop::collection::vec(feature(), n), -3.0..3.0_f64);
        (Just(config), prop::collection::vec(example, steps))
    })
}

fn trained(config: Config, examples: &[(Vec<f64>, f64)]) -> Idbd {
    let mut learner = Idbd::new(examples[0].0.len(), config).expect("valid learner");
    for (x, target) in examples {
        learner.update(x, *target).expect("generated update");
    }
    learner
}

#[derive(Debug, Clone, Copy)]
enum Fault {
    ExtraFeature,
    NonFiniteFeature(usize, f64),
    NonFiniteTarget(f64),
}

fn fault() -> impl Strategy<Value = Fault> {
    let non_finite = prop_oneof![Just(f64::NAN), Just(f64::INFINITY), Just(f64::NEG_INFINITY)];
    prop_oneof![
        Just(Fault::ExtraFeature),
        (any::<usize>(), non_finite.clone()).prop_map(|(i, v)| Fault::NonFiniteFeature(i, v)),
        non_finite.prop_map(Fault::NonFiniteTarget),
    ]
}

proptest! {
    #[test]
    fn matches_a_naive_transcription_of_the_paper((config, examples) in scenario(50)) {
        let n = examples[0].0.len();
        let mut learner = Idbd::new(n, config).expect("valid learner");
        let mut naive = Naive::new(n, &config);
        for (t, (x, target)) in examples.iter().enumerate() {
            let report = learner.update(x, *target).expect("generated update");
            let (y, delta) = naive.update(x, *target);
            let at = format!("step {t}");
            assert_close(report.prediction, y, &at);
            assert_close(report.error, delta, &at);
            assert_all_close(learner.weights(), &naive.w, &at);
            assert_all_close(learner.log_step_sizes(), &naive.beta, &at);
            assert_all_close(learner.traces(), &naive.h, &at);
        }
    }

    #[test]
    fn failed_operations_never_change_committed_state(
        (config, examples) in scenario(10),
        fault in fault(),
    ) {
        let mut learner = trained(config, &examples);
        let before = learner.snapshot();
        let mut x = examples[0].0.clone();
        let mut target = 1.0;
        let expected = match fault {
            Fault::ExtraFeature => {
                x.push(1.0);
                Error::DimensionMismatch { expected: x.len() - 1, found: x.len() }
            }
            Fault::NonFiniteFeature(i, value) => {
                let index = i % x.len();
                x[index] = value;
                Error::NonFiniteFeature { index }
            }
            Fault::NonFiniteTarget(value) => {
                target = value;
                Error::NonFiniteTarget
            }
        };
        prop_assert_eq!(learner.update(&x, target), Err(expected));
        assert_same_state(&learner.snapshot(), &before);
    }

    #[test]
    fn zero_features_leave_their_coordinates_unchanged((config, examples) in scenario(20)) {
        let mut learner = Idbd::new(examples[0].0.len(), config).expect("valid learner");
        for (x, target) in &examples {
            let before = learner.snapshot();
            learner.update(x, *target).expect("generated update");
            for (i, _) in x.iter().enumerate().filter(|(_, xi)| **xi == 0.0) {
                prop_assert_eq!(learner.weights()[i].to_bits(), before.weights[i].to_bits());
                prop_assert_eq!(learner.log_step_sizes()[i].to_bits(), before.betas[i].to_bits());
                prop_assert_eq!(learner.traces()[i].to_bits(), before.traces[i].to_bits());
            }
        }
    }

    #[test]
    fn snapshot_round_trip_is_bitwise_and_prediction_is_pure(
        (config, examples) in scenario(20),
    ) {
        let learner = trained(config, &examples);
        let snapshot = learner.snapshot();
        let restored = Idbd::from_state(snapshot.clone()).expect("own snapshot is valid");
        assert_same_state(&restored.snapshot(), &snapshot);
        let x = &examples[0].0;
        let a = learner.predict(x).expect("prediction");
        let b = restored.predict(x).expect("prediction");
        prop_assert_eq!(a.to_bits(), b.to_bits());
        assert_same_state(&learner.snapshot(), &snapshot);
    }

    #[test]
    fn betas_stay_within_their_bounds((config, examples) in scenario(50)) {
        let mut learner = Idbd::new(examples[0].0.len(), config).expect("valid learner");
        for (x, target) in &examples {
            let report = learner.update(x, *target).expect("generated update");
            prop_assert!(report.bounded_features <= x.len());
            for &beta in learner.log_step_sizes() {
                prop_assert!(config.bounds().contains(beta), "beta {} outside bounds", beta);
            }
        }
    }
}
