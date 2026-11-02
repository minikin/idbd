//! Rule: Every update follows the IDBD recurrence.

use idbd::Idbd;

use crate::fixtures::{C06_C07, Fixture, SEQUENCE_BOUNDED, SEQUENCE_UNBOUNDED};
use crate::support::{TestRng, assert_all_close, assert_close, config, restore, standard_learner};

fn replay(fixture: &Fixture, name: &str) {
    let mut learner = Idbd::new(fixture.num_features(), fixture.config()).expect("fixture learner");
    for (t, step) in fixture.steps.iter().enumerate() {
        let report = learner
            .update(step.features, step.target)
            .expect("fixture update succeeds");
        let at = format!("{name} step {t}");
        assert_close(
            report.prediction,
            step.prediction,
            &format!("{at} prediction"),
        );
        assert_close(report.error, step.error, &format!("{at} error"));
        assert_eq!(
            report.bounded_features, step.bounded_features,
            "{at} bounded"
        );
        assert_all_close(learner.weights(), step.weights, &format!("{at} weights"));
        assert_all_close(learner.log_step_sizes(), step.betas, &format!("{at} betas"));
        assert_all_close(learner.traces(), step.traces, &format!("{at} traces"));
    }
}

#[test]
fn c06_learn_the_first_example_from_zero_traces() {
    let mut learner = standard_learner();
    let report = learner.update(&[1.0, 2.0], 1.0).expect("update");

    assert_eq!(report.prediction, 0.0);
    assert_eq!(report.error, 1.0);
    assert_all_close(learner.weights(), &[0.1, 0.2], "weights");
    assert_all_close(learner.traces(), &[0.1, 0.2], "traces");
    let beta = 0.1_f64.ln();
    assert_eq!(learner.log_step_sizes(), &[beta, beta], "betas unchanged");
    assert_eq!(report.bounded_features, 0);
}

#[test]
fn c07_use_old_traces_and_one_shared_pre_update_error() {
    let mut learner = standard_learner();
    learner.update(&[1.0, 2.0], 1.0).expect("C06 update");
    let report = learner.update(&[2.0, -1.0], 0.5).expect("C07 update");

    assert_close(report.prediction, 0.0, "prediction");
    assert_close(report.error, 0.5, "error");
    let beta = 0.1_f64.ln();
    assert_all_close(
        learner.log_step_sizes(),
        &[beta + 0.001, beta - 0.001],
        "betas",
    );
    let alphas: Vec<f64> = learner.step_sizes().collect();
    assert_all_close(
        &alphas,
        &[0.100_100_050_016_670_84, 0.099_900_049_983_337_51],
        "alphas",
    );
    assert_all_close(
        learner.weights(),
        &[0.200_100_050_016_670_86, 0.150_049_975_008_331_25],
        "weights",
    );
    assert_all_close(
        learner.traces(),
        &[0.160_060_030_010_002_52, 0.130_069_965_011_663_76],
        "traces",
    );
}

#[test]
fn c06_c07_match_the_independent_reference() {
    replay(&C06_C07, "C06_C07");
}

#[test]
fn reference_sequence_without_bounds() {
    replay(&SEQUENCE_UNBOUNDED, "SEQUENCE_UNBOUNDED");
}

#[test]
fn reference_sequence_with_bounds() {
    replay(&SEQUENCE_BOUNDED, "SEQUENCE_BOUNDED");
}

#[test]
fn c08_disable_meta_learning_explicitly() {
    let mut rng = TestRng::new(8);
    let betas: Vec<f64> = [0.02, 0.04, 0.06, 0.08]
        .iter()
        .map(|a: &f64| a.ln())
        .collect();
    let mut idbd = restore(config(0.05, 0.0), &[0.0; 4], &betas, &[0.0; 4]);
    let rates: Vec<f64> = betas.iter().map(|b| b.exp()).collect();
    let mut lms_weights = vec![0.0; 4];

    for _ in 0..500 {
        let x: Vec<f64> = (0..4).map(|_| rng.symmetric(2.0)).collect();
        let target = 1.5 * x[0] - 0.5 * x[2] + rng.symmetric(0.1);

        let lms_prediction: f64 = lms_weights.iter().zip(&x).map(|(w, xi)| w * xi).sum();
        let lms_error = target - lms_prediction;
        for ((w, rate), xi) in lms_weights.iter_mut().zip(&rates).zip(&x) {
            *w += rate * lms_error * xi;
        }

        let report = idbd.update(&x, target).expect("update");
        assert_close(report.prediction, lms_prediction, "prediction");
        assert_close(report.error, lms_error, "error");
        assert_all_close(idbd.weights(), &lms_weights, "weights");
        assert_eq!(idbd.log_step_sizes(), betas.as_slice(), "betas unchanged");
    }
}

#[test]
fn c09_preserve_an_inactive_coordinate() {
    let beta = 0.2_f64.ln();
    let mut learner = restore(
        config(0.2, 0.05),
        &[0.4, -1.3, 0.9],
        &[beta, beta - 0.3, beta + 0.1],
        &[0.25, -0.6, 0.15],
    );
    let before = learner.snapshot();

    learner.update(&[1.0, 0.0, -2.0], 3.0).expect("update");

    assert_eq!(learner.weights()[1].to_bits(), before.weights[1].to_bits());
    assert_eq!(
        learner.log_step_sizes()[1].to_bits(),
        before.betas[1].to_bits()
    );
    assert_eq!(learner.traces()[1].to_bits(), before.traces[1].to_bits());
    assert_ne!(
        learner.weights()[0],
        before.weights[0],
        "active coordinate learns"
    );
}

#[test]
fn c09_inactive_coordinate_survives_an_overflowing_step() {
    // alpha * error overflows, so computing `* 0.0` would give NaN.
    let mut learner = restore(config(0.1, 0.0), &[0.0, 0.0], &[700.0, 0.0], &[0.0, 0.0]);
    let before = learner.snapshot();
    learner
        .update(&[0.0, 1.0], 1e300)
        .expect("inactive coordinate is skipped");
    assert_eq!(learner.weights()[0].to_bits(), before.weights[0].to_bits());
}

fn one_feature_after_learning_two() -> Idbd {
    let mut learner = Idbd::new(1, config(0.5, 0.01)).expect("valid learner");
    learner.update(&[1.0], 2.0).expect("learn target 2");
    learner
}

#[test]
fn c10_decay_the_trace_even_when_the_prediction_is_correct() {
    let mut learner = one_feature_after_learning_two();
    let beta = learner.log_step_sizes()[0];

    let report = learner.update(&[1.0], 1.0).expect("update");

    assert_eq!(report.error, 0.0);
    assert_eq!(learner.weights(), &[1.0]);
    assert_eq!(learner.log_step_sizes(), &[beta]);
    assert_close(learner.traces()[0], 0.5, "trace");
}

#[test]
fn c11_clamp_negative_trace_decay_to_zero() {
    let mut learner = one_feature_after_learning_two();
    let beta = learner.log_step_sizes()[0];

    let report = learner.update(&[2.0], 2.0).expect("update");

    assert_eq!(report.error, 0.0);
    assert_eq!(learner.weights(), &[1.0]);
    assert_eq!(learner.log_step_sizes(), &[beta]);
    assert_eq!(learner.traces(), &[0.0]);
}

#[test]
fn update_reports_prediction_made_before_learning() {
    let mut learner = standard_learner();
    learner.update(&[1.0, 2.0], 1.0).expect("update");
    let expected = learner.predict(&[0.5, -1.0]).expect("prediction");
    let report = learner.update(&[0.5, -1.0], 2.0).expect("update");
    assert_eq!(report.prediction.to_bits(), expected.to_bits());
    assert_eq!(report.error.to_bits(), (2.0 - expected).to_bits());
}
