//! Rule: Prediction and inspection describe committed state without mutation.

use idbd::{Error, Idbd};

use crate::support::{assert_close, assert_same_state, config, restore, standard_learner};

#[test]
fn c03_predict_a_known_linear_result() {
    let beta = 0.1_f64.ln();
    let learner = restore(config(0.1, 0.01), &[2.0, -1.0, 0.5], &[beta; 3], &[0.0; 3]);
    let before = learner.snapshot();

    let prediction = learner
        .predict(&[3.0, 4.0, 2.0])
        .expect("finite prediction");

    assert_close(prediction, 3.0, "prediction");
    assert_same_state(&learner.snapshot(), &before);
}

#[test]
fn c04_inspect_the_state_without_copying_it() {
    let mut learner = standard_learner();
    learner.update(&[1.0, 2.0], 1.0).expect("first update");
    let before = learner.snapshot();

    assert_eq!(learner.num_features(), 2);
    assert_eq!(learner.config(), &before.config);
    assert_eq!(learner.weights(), before.weights.as_slice());
    assert_eq!(learner.log_step_sizes(), before.betas.as_slice());
    assert_eq!(learner.traces(), before.traces.as_slice());

    let step_sizes = learner.step_sizes();
    assert_eq!(step_sizes.len(), 2);
    assert_eq!(learner.step_sizes().take(3).count(), 2, "the iterator ends");
    let mut count = 0;
    for (alpha, beta) in step_sizes.zip(learner.log_step_sizes()) {
        assert_eq!(alpha.to_bits(), beta.exp().to_bits(), "alpha = exp(beta)");
        count += 1;
    }
    assert_eq!(count, 2);
    let reversed = learner.step_sizes().next_back().expect("last step size");
    assert_eq!(reversed.to_bits(), before.betas[1].exp().to_bits());

    assert_same_state(&learner.snapshot(), &before);
}

#[test]
fn c05_reject_an_invalid_feature_vector() {
    let cases: [(&str, &[f64], Error); 5] = [
        (
            "only 2 elements",
            &[1.0, 2.0],
            Error::DimensionMismatch {
                expected: 3,
                found: 2,
            },
        ),
        (
            "4 elements",
            &[1.0, 2.0, 3.0, 4.0],
            Error::DimensionMismatch {
                expected: 3,
                found: 4,
            },
        ),
        (
            "a NaN element",
            &[1.0, f64::NAN, 3.0],
            Error::NonFiniteFeature { index: 1 },
        ),
        (
            "a positive infinity",
            &[f64::INFINITY, 2.0, 3.0],
            Error::NonFiniteFeature { index: 0 },
        ),
        (
            "a negative infinity",
            &[1.0, 2.0, f64::NEG_INFINITY],
            Error::NonFiniteFeature { index: 2 },
        ),
    ];
    let mut learner = trained_three_feature_learner();
    let before = learner.snapshot();
    for (fault, features, expected) in cases {
        assert_eq!(learner.predict(features), Err(expected), "predict, {fault}");
        assert_same_state(&learner.snapshot(), &before);
        assert_eq!(
            learner.update(features, 1.0),
            Err(expected),
            "update with finite target, {fault}"
        );
        assert_same_state(&learner.snapshot(), &before);
    }
}

fn trained_three_feature_learner() -> Idbd {
    let mut learner = Idbd::new(3, config(0.05, 0.01)).expect("valid learner");
    learner.update(&[1.0, -0.5, 2.0], 0.7).expect("update");
    learner.update(&[0.3, 1.5, -1.0], -0.2).expect("update");
    learner
}
