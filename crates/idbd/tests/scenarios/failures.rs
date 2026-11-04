//! Rule: Failed operations preserve all committed learning state.

use idbd::{Bounds, Error, Idbd, Stage};

use crate::support::{assert_same_state, config, restore, safety_config, standard_learner};

fn numerical(stage: Stage, coordinate: Option<usize>) -> Error {
    Error::Numerical { stage, coordinate }
}

/// Runs `operation`, asserts it returns `expected`, and asserts the committed state did
/// not change.
#[track_caller]
fn assert_rejected<T: std::fmt::Debug + PartialEq>(
    learner: &mut Idbd,
    operation: impl FnOnce(&mut Idbd) -> Result<T, Error>,
    expected: Error,
) {
    let before = learner.snapshot();
    let result = operation(learner);
    assert_eq!(result, Err(expected), "operation result");
    assert_same_state(&learner.snapshot(), &before);
}

#[test]
fn c15_reject_a_non_finite_target() {
    for target in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut learner = standard_learner();
        learner.update(&[1.0, 2.0], 1.0).expect("prior update");
        assert_rejected(
            &mut learner,
            |l| l.update(&[0.5, 0.5], target),
            Error::NonFiniteTarget,
        );
    }
}

#[test]
fn c16_reject_an_overflowing_prediction() {
    let beta = 0.1_f64.ln();
    let mut learner = restore(config(0.1, 0.01), &[1e308, 1e308], &[beta; 2], &[0.0; 2]);
    let overflow = numerical(Stage::Prediction, None);
    assert_rejected(&mut learner, |l| l.predict(&[1.0, 1.0]), overflow);
    assert_rejected(&mut learner, |l| l.update(&[1.0, 1.0], 0.0), overflow);
}

#[test]
fn c17_reject_finite_inputs_whose_residual_overflows() {
    let beta = 0.1_f64.ln();
    let mut learner = restore(config(0.1, 0.01), &[1e308], &[beta], &[0.0]);
    assert_eq!(learner.predict(&[1.0]), Ok(1e308), "finite prediction");
    assert_rejected(
        &mut learner,
        |l| l.update(&[1.0], -1e308),
        numerical(Stage::Residual, None),
    );
}

#[test]
fn c18_reject_a_late_weight_failure_without_committing_earlier_coordinates() {
    let beta = 0.1_f64.ln();
    let mut learner = restore(
        config(0.1, 0.0),
        &[0.3, -0.2, 0.0],
        &[beta, beta, 1e300_f64.ln()],
        &[0.1, 0.2, 0.0],
    );
    assert_rejected(
        &mut learner,
        |l| l.update(&[1.0, 1.0, 1.0], 1e10),
        numerical(Stage::Weight, Some(2)),
    );
}

#[test]
fn c18_reject_a_late_trace_failure_without_committing_earlier_coordinates() {
    let beta = 0.1_f64.ln();
    let mut learner = restore(
        config(0.1, 0.0),
        &[0.0, 0.0],
        &[beta, 1e10_f64.ln()],
        &[0.5, f64::MAX],
    );
    // The step is about 1e293 and the decay rounds to 1, so only the trace overflows.
    assert_rejected(
        &mut learner,
        |l| l.update(&[1.0, 1e-15], 1e298),
        numerical(Stage::Trace, Some(1)),
    );
}

#[test]
fn c19_reject_a_step_size_that_underflows_to_zero() {
    let mut learner = restore(config(0.05, 1.0), &[0.0], &[-700.0], &[-100.0]);
    assert_rejected(
        &mut learner,
        |l| l.update(&[1.0], 1.0),
        numerical(Stage::StepSize, Some(0)),
    );
}

#[test]
fn c19_reject_a_step_size_that_overflows_to_infinity() {
    let mut learner = restore(config(0.05, 1.0), &[0.0], &[700.0], &[20.0]);
    assert_rejected(
        &mut learner,
        |l| l.update(&[1.0], 1.0),
        numerical(Stage::StepSize, Some(0)),
    );
}

#[test]
fn c19_bounds_cannot_clamp_beta_onto_an_unrepresentable_step_size() {
    // A floor below ln(f64::MIN_POSITIVE * 2^-52) or a ceiling above ln(f64::MAX) would
    // clamp beta onto a zero or infinite step size, so configuration rejects them.
    assert!(
        Bounds::none().with_min_beta(-1000.0).is_err(),
        "underflowing floor"
    );
    assert!(
        Bounds::none().with_max_beta(710.0).is_err(),
        "overflowing ceiling"
    );

    // The lowest accepted floor still yields a positive step size when beta reaches it.
    let floor = Bounds::none()
        .with_min_beta(-745.0)
        .expect("representable floor");
    let config = config(0.05, 1.0).with_bounds(floor).expect("valid config");
    let mut learner = restore(config, &[0.0], &[-700.0], &[-100.0]);
    let report = learner.update(&[1.0], 1.0).expect("floored update");
    assert_eq!(learner.log_step_sizes(), &[-745.0]);
    assert!(
        learner.step_sizes().all(|alpha| alpha > 0.0),
        "positive step size"
    );
    assert_eq!(report.bounded_features, 1);
}

#[test]
fn c20_do_not_use_clipping_to_conceal_invalid_arithmetic() {
    let mut learner = restore(safety_config(0.05, 1.0), &[0.0], &[-3.0], &[1e300]);
    assert_rejected(
        &mut learner,
        |l| l.update(&[1.0], 1e10),
        numerical(Stage::MetaStep, Some(0)),
    );
}

#[test]
fn failed_update_then_success_matches_a_learner_that_never_failed() {
    let mut failed = standard_learner();
    let mut clean = standard_learner();
    failed.update(&[1.0, 2.0], 1.0).expect("update");
    clean.update(&[1.0, 2.0], 1.0).expect("update");
    assert!(failed.update(&[1.0, 2.0], f64::NAN).is_err(), "rejected");
    let a = failed
        .update(&[2.0, -1.0], 0.5)
        .expect("update after failure");
    let b = clean.update(&[2.0, -1.0], 0.5).expect("update");
    assert_eq!(a, b);
    assert_same_state(&failed.snapshot(), &clean.snapshot());
}

#[test]
fn c20_an_overflowing_decay_product_clamps_to_its_exact_value() {
    // alpha * x * x = 1e400 overflows, and [1 - 1e400]^+ is exactly 0.
    let mut learner = Idbd::new(1, config(1.0, 0.0)).expect("valid");
    learner
        .update(&[1e200], 1.0)
        .expect("the exact update is finite");
    assert_eq!(learner.weights(), &[1e200]);
    assert_eq!(learner.traces(), &[1e200]);
}
