//! A01 to A07: Autostep, Table 1 of Mahmood, Sutton, Degris and Pilarski (2012).

use idbd::autostep::{Autostep, Config, State, StateError};
use idbd::{ConfigError, Error, Stage};
use proptest::prelude::*;

use crate::fixtures::{AUTOSTEP_DEFAULTS, AUTOSTEP_FAST, AutostepFixture};
use crate::support::{assert_all_close, assert_close};

fn learner(num_features: usize, initial_step_size: f64) -> Autostep {
    Autostep::new(
        num_features,
        Config::new(initial_step_size).expect("valid step size"),
    )
    .expect("nonzero features")
}

fn bits(learner: &Autostep) -> Vec<u64> {
    let state = learner.snapshot();
    state
        .weights
        .iter()
        .chain(&state.step_sizes)
        .chain(&state.traces)
        .chain(&state.normalizers)
        .map(|v| v.to_bits())
        .collect()
}

fn check_against(fixture: &AutostepFixture) {
    let mut learner =
        Autostep::new(fixture.num_features(), fixture.config()).expect("valid fixture");
    for (k, step) in fixture.steps.iter().enumerate() {
        let report = learner
            .update(step.features, step.target)
            .expect("fixture step succeeds");
        assert_close(
            report.prediction,
            step.prediction,
            &format!("step {k} prediction"),
        );
        assert_close(report.error, step.error, &format!("step {k} error"));
        assert_close(
            report.effective_step_size,
            step.effective_step_size,
            &format!("step {k} effective step size"),
        );
        assert_all_close(
            learner.weights(),
            step.weights,
            &format!("step {k} weights"),
        );
        assert_all_close(
            learner.step_sizes(),
            step.step_sizes,
            &format!("step {k} step sizes"),
        );
        assert_all_close(learner.traces(), step.traces, &format!("step {k} traces"));
        assert_all_close(
            learner.normalizers(),
            step.normalizers,
            &format!("step {k} normalizers"),
        );
    }
}

#[test]
fn a01_match_the_independent_reference_with_the_paper_defaults() {
    assert!(
        AUTOSTEP_DEFAULTS
            .steps
            .iter()
            .any(|step| step.effective_step_size > 1.0),
        "the fixture exercises the overshoot guard"
    );
    check_against(&AUTOSTEP_DEFAULTS);
}

#[test]
fn a01_match_the_independent_reference_with_a_fast_configuration() {
    check_against(&AUTOSTEP_FAST);
}

#[test]
fn a02_construct_with_the_paper_defaults() {
    let config = Config::new(0.1).expect("valid step size");
    assert_eq!(config.meta_step_size(), 0.01);
    assert_eq!(config.time_scale(), 10_000.0);
    let learner = Autostep::new(3, config).expect("valid learner");
    assert_eq!(learner.num_features(), 3);
    assert_eq!(learner.weights(), [0.0; 3]);
    assert_eq!(learner.step_sizes(), [0.1; 3]);
    assert_eq!(learner.traces(), [0.0; 3]);
    assert_eq!(learner.normalizers(), [0.0; 3]);
    assert_eq!(learner.config(), &config);
}

#[test]
fn a02_reject_invalid_configuration() {
    for value in [0.0, -0.1, f64::NAN, f64::INFINITY] {
        assert!(
            matches!(
                Config::new(value),
                Err(ConfigError::InvalidInitialStepSize { .. })
            ),
            "{value}"
        );
    }
    let config = Config::new(0.1).expect("valid step size");
    for value in [-0.01, f64::NAN, f64::INFINITY] {
        assert!(
            matches!(
                config.with_meta_step_size(value),
                Err(ConfigError::InvalidMetaStepSize { .. })
            ),
            "{value}"
        );
    }
    assert_eq!(
        config.with_meta_step_size(0.0).map(|c| c.meta_step_size()),
        Ok(0.0)
    );
    for value in [0.999, 0.0, -1.0, f64::NAN, f64::INFINITY] {
        assert!(
            matches!(
                config.with_time_scale(value),
                Err(ConfigError::InvalidTimeScale { .. })
            ),
            "{value}"
        );
    }
    assert_eq!(config.with_time_scale(1.0).map(|c| c.time_scale()), Ok(1.0));
    assert_eq!(
        Autostep::new(0, config).map(|l| l.num_features()),
        Err(ConfigError::ZeroFeatures)
    );
}

#[test]
fn a03_the_overshoot_guard_divides_every_step_size_by_the_effective_step_size() {
    let mut learner = learner(3, 0.5);
    let report = learner.update(&[2.0, 0.0, 1.0], 1.0).expect("finite");
    // First step: every trace is zero, so the normalizers stay zero and no step size
    // adapts. sum(alpha x^2) = 0.5 * 4 + 0.5 * 1 = 2.5.
    assert_eq!(report.effective_step_size, 2.5);
    assert_eq!(learner.step_sizes(), [0.2, 0.2, 0.2]);
    assert_eq!(learner.normalizers(), [0.0; 3]);
    assert_eq!(learner.weights()[1], 0.0, "a zero input keeps its weight");
    assert_eq!(learner.traces()[1], 0.0, "and its trace");
    let after = learner.predict(&[2.0, 0.0, 1.0]).expect("finite");
    assert_close(
        after,
        1.0,
        "an effective step size of one removes the error",
    );
}

#[test]
fn a03_an_effective_step_size_below_one_leaves_step_sizes_unscaled() {
    let mut learner = learner(2, 0.1);
    let report = learner.update(&[1.0, 2.0], 1.0).expect("finite");
    assert_close(report.effective_step_size, 0.5, "0.1 + 0.1 * 4");
    assert_eq!(learner.step_sizes(), [0.1, 0.1]);
}

fn restored(mu: f64, step_size: f64, trace: f64) -> Autostep {
    let config = Config::new(0.1)
        .and_then(|c| c.with_meta_step_size(mu))
        .expect("valid");
    Autostep::from_state(
        State::new(config, vec![0.0, 0.0])
            .with_step_sizes(vec![step_size, 0.1])
            .with_traces(vec![trace, 0.0]),
    )
    .expect("valid state")
}

#[track_caller]
fn assert_fails_unchanged(mut learner: Autostep, features: &[f64], target: f64, expected: Error) {
    let before = bits(&learner);
    let result = learner.update(features, target).map(|_| ());
    assert_eq!(result, Err(expected), "the update reports {expected}");
    assert_eq!(bits(&learner), before, "state after {expected}");
}

const fn at(stage: Stage, coordinate: Option<usize>) -> Error {
    Error::Numerical { stage, coordinate }
}

#[test]
fn a04_invalid_input_fails_without_changing_state() {
    let mut trained = learner(2, 0.1);
    trained.update(&[1.0, 2.0], 1.0).expect("finite");
    trained.update(&[0.5, -1.0], 0.3).expect("finite");
    let cases: [(&[f64], f64, Error); 3] = [
        (
            &[1.0],
            0.0,
            Error::DimensionMismatch {
                expected: 2,
                found: 1,
            },
        ),
        (&[1.0, f64::NAN], 0.0, Error::NonFiniteFeature { index: 1 }),
        (&[1.0, 1.0], f64::INFINITY, Error::NonFiniteTarget),
    ];
    for (features, target, expected) in cases {
        assert_fails_unchanged(trained.clone(), features, target, expected);
    }
    let unit_weights = Autostep::from_state(
        State::new(Config::new(0.1).expect("valid"), vec![1.0, 1.0])
            .with_step_sizes(vec![0.1, 0.1]),
    )
    .expect("valid state");
    assert_fails_unchanged(
        unit_weights,
        &[1e308, 1e308],
        0.0,
        at(Stage::Prediction, None),
    );
}

#[test]
fn a04_each_numerical_stage_fails_without_changing_state() {
    // delta * x_0 * h_0 = f64::MAX * 1 * 10 overflows.
    assert_fails_unchanged(
        restored(0.01, 0.1, 10.0),
        &[1.0, 0.0],
        f64::MAX,
        at(Stage::MetaStep, Some(0)),
    );
    // (1 / tau) * alpha_0 * x_0 * x_0 * |g| = 1e-4 * 1e300 * 1e10 * 1e5 overflows.
    assert_fails_unchanged(
        restored(0.01, 1e300, 1.0),
        &[1e5, 0.0],
        1.0,
        at(Stage::Normalizer, Some(0)),
    );
    // |g| / v_0 = 1, so alpha_0 grows by exp(0.5) past f64::MAX.
    assert_fails_unchanged(
        restored(0.5, f64::MAX, 1.0),
        &[1e-200, 0.0],
        1.0,
        at(Stage::StepSize, Some(0)),
    );
    // alpha_0 * x_0 * x_0 = 1e300 * 1e10 * 1e10 overflows.
    assert_fails_unchanged(
        restored(0.01, 1e300, 0.0),
        &[1e10, 0.0],
        0.0,
        at(Stage::EffectiveStepSize, None),
    );
    // delta = -f64::MAX - f64::MAX overflows.
    let unit_weight = Autostep::from_state(State::new(Config::new(0.1).expect("valid"), vec![1.0]))
        .expect("valid state");
    assert_fails_unchanged(
        unit_weight,
        &[f64::MAX],
        -f64::MAX,
        at(Stage::Residual, None),
    );
    // The guard divides by M = 1e300, so alpha_1 = 1e-300 / 1e300 underflows to zero.
    assert_fails_unchanged(
        Autostep::from_state(
            State::new(Config::new(0.1).expect("valid"), vec![0.0, 0.0])
                .with_step_sizes(vec![1e300, 1e-300]),
        )
        .expect("valid state"),
        &[1.0, 1.0],
        0.0,
        at(Stage::StepSize, Some(1)),
    );
    // sum(alpha x^2) = 1e10 * 1e-20 stays below one, but alpha_0 * delta overflows.
    assert_fails_unchanged(
        restored(0.01, 1e10, 0.0),
        &[1e-10, 0.0],
        1e308,
        at(Stage::Weight, Some(0)),
    );
}

#[test]
fn a05_resume_from_a_snapshot_bit_for_bit() {
    let mut original = learner(3, 0.1);
    for step in AUTOSTEP_FAST.steps.iter().take(4) {
        original.update(step.features, step.target).expect("finite");
    }
    let mut resumed = Autostep::from_state(original.snapshot()).expect("valid snapshot");
    assert_eq!(bits(&resumed), bits(&original));
    for step in AUTOSTEP_FAST.steps.iter().skip(4) {
        let a = original.update(step.features, step.target).expect("finite");
        let b = resumed.update(step.features, step.target).expect("finite");
        assert_eq!(a, b);
    }
    assert_eq!(bits(&resumed), bits(&original));
    assert_eq!(
        Autostep::try_from(original.snapshot()).map(|l| bits(&l)),
        Ok(bits(&original))
    );
}

#[test]
fn a05_reject_an_invalid_snapshot() {
    let config = Config::new(0.1).expect("valid step size");
    let valid = || {
        (
            vec![0.0, 1.0],
            vec![0.1, 0.2],
            vec![0.0, -0.5],
            vec![0.0, 0.3],
        )
    };
    let state = |(w, a, h, v): (Vec<f64>, Vec<f64>, Vec<f64>, Vec<f64>)| {
        Autostep::from_state(
            State::new(config, w)
                .with_step_sizes(a)
                .with_traces(h)
                .with_normalizers(v),
        )
        .map(|_| ())
    };
    assert_eq!(state(valid()), Ok(()));
    assert_eq!(
        state((vec![], vec![], vec![], vec![])),
        Err(StateError::Empty)
    );
    let (w, a, h, _) = valid();
    assert_eq!(
        state((w, a, h, vec![0.0])),
        Err(StateError::LengthMismatch {
            weights: 2,
            step_sizes: 2,
            traces: 2,
            normalizers: 1
        })
    );
    let mut bad = valid();
    bad.0[1] = f64::NAN;
    assert_eq!(state(bad), Err(StateError::NonFiniteWeight { index: 1 }));
    for step_size in [0.0, -0.1, f64::INFINITY] {
        let mut bad = valid();
        bad.1[0] = step_size;
        assert_eq!(
            state(bad),
            Err(StateError::InvalidStepSize {
                index: 0,
                step_size
            })
        );
    }
    let mut bad = valid();
    bad.2[1] = f64::NEG_INFINITY;
    assert_eq!(state(bad), Err(StateError::NonFiniteTrace { index: 1 }));
    for normalizer in [-1e-300, f64::INFINITY] {
        let mut bad = valid();
        bad.3[1] = normalizer;
        assert_eq!(
            state(bad),
            Err(StateError::InvalidNormalizer {
                index: 1,
                normalizer
            })
        );
    }
}

#[test]
fn a06_learn_raw_unscaled_targets_without_tuning() {
    // Targets in raw milliseconds, between 40 and 192, with the paper's defaults and
    // the initial step size 0.1 / n.
    let mut learner = learner(3, 0.1 / 3.0);
    let mut last_error = f64::INFINITY;
    for t in 0..2_000_u32 {
        let load = f64::from(t % 17) / 17.0;
        let size = f64::from(t % 5);
        let target = 40.0 + 120.0 * load + 8.0 * size;
        let report = learner.update(&[1.0, load, size], target).expect("finite");
        last_error = report.error;
    }
    assert!(
        last_error.abs() < 1.0,
        "error after 2,000 samples: {last_error}"
    );
}

proptest! {
    #[test]
    fn a07_no_update_overshoots_and_step_sizes_change_by_at_most_exp_mu(
        features in prop::collection::vec(-10.0_f64..10.0, 1..8),
        targets in prop::collection::vec(-100.0_f64..100.0, 1..40),
        initial in 1e-4_f64..2.0,
        mu in 0.0_f64..0.5,
    ) {
        let config = Config::new(initial)
            .and_then(|c| c.with_meta_step_size(mu))
            .and_then(|c| c.with_time_scale(10.0))
            .expect("valid");
        let mut learner = Autostep::new(features.len(), config).expect("valid");
        for target in targets {
            let old: Vec<f64> = learner.step_sizes().to_vec();
            let report = learner.update(&features, target).expect("finite");
            let after = target - learner.predict(&features).expect("finite");
            // delta' = delta * (1 - sum(alpha x^2)) with the guarded step sizes, which
            // lies between 0 and delta.
            let tolerance = 1e-9 * (1.0 + report.error.abs());
            prop_assert!(after.abs() <= report.error.abs() + tolerance);
            prop_assert!(after * report.error >= -tolerance * report.error.abs());
            let m = report.effective_step_size.max(1.0);
            for (new, old) in learner.step_sizes().iter().zip(&old) {
                let ratio = new * m / old;
                prop_assert!(ratio <= mu.exp() * (1.0 + 1e-12));
                prop_assert!(ratio >= (-mu).exp() * (1.0 - 1e-12));
            }
            prop_assert!(learner.normalizers().iter().all(|&v| v >= 0.0));
        }
    }
}

#[test]
fn a05_clone_from_copies_a_learner_of_any_size() {
    let config = Config::new(0.2)
        .and_then(|c| c.with_meta_step_size(0.05))
        .expect("valid");
    let mut source = Autostep::new(3, config).expect("valid");
    source.update(&[1.0, -0.5, 2.0], 0.7).expect("finite");
    let mut target = learner(5, 0.1);
    target.clone_from(&source);
    assert_eq!(target.config(), source.config());
    assert_eq!(bits(&target), bits(&source));
    let a = source.update(&[0.3, 0.1, -1.0], 0.2).expect("finite");
    let b = target
        .update(&[0.3, 0.1, -1.0], 0.2)
        .expect("the candidate buffer was resized");
    assert_eq!(a, b);
    assert_eq!(bits(&target), bits(&source));
}

#[test]
fn a04_an_overflowing_normalizer_term_gives_its_exact_value() {
    // |g| = 1e154 < v = 1e308, so the decayed term is 1e308 * (1e154 - 1e308), which
    // overflows to -inf, and the normalizer is exactly max(|g|, that) = |g|.
    let config = Config::new(0.1)
        .and_then(|c| c.with_time_scale(1.0))
        .expect("valid");
    let mut learner = Autostep::from_state(
        State::new(config, vec![0.0])
            .with_step_sizes(vec![1.0])
            .with_traces(vec![-1.0])
            .with_normalizers(vec![1e308]),
    )
    .expect("valid state");
    learner
        .update(&[1e154], 1.0)
        .expect("the exact update is finite");
    assert_eq!(learner.normalizers(), &[1e154]);
}

#[test]
fn a05_a_hand_built_state_names_every_vector() {
    let config = Config::new(0.05).expect("valid");
    let fresh = Autostep::new(2, config).expect("valid").snapshot();
    assert_eq!(State::new(config, vec![0.0, 0.0]), fresh);
    let state = State::new(config, vec![1.0, 2.0])
        .with_step_sizes(vec![0.1, 0.2])
        .with_traces(vec![0.3, 0.4])
        .with_normalizers(vec![0.5, 0.6]);
    assert_eq!(state.step_sizes, [0.1, 0.2]);
    assert_eq!(state.traces, [0.3, 0.4]);
    assert_eq!(state.normalizers, [0.5, 0.6]);
}
