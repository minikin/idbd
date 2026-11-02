//! Rule: A complete snapshot continues the same learning process.

use idbd::{Config, Idbd, State, StateError};

use crate::support::{TestRng, assert_same_state, config, safety_config};

fn example(rng: &mut TestRng) -> ([f64; 5], f64) {
    let x = [
        rng.symmetric(2.0),
        rng.symmetric(2.0),
        rng.symmetric(2.0),
        rng.symmetric(2.0),
        1.0,
    ];
    let target = 2.0 * x[0] - x[1] + 0.3 + rng.symmetric(0.2);
    (x, target)
}

#[test]
fn c21_resume_without_resetting_adaptation() {
    let mut rng = TestRng::new(21);
    let mut original = Idbd::new(5, config(0.05, 0.02)).expect("valid learner");
    for _ in 0..2_000 {
        let (x, target) = example(&mut rng);
        original.update(&x, target).expect("prefix update");
    }

    let snapshot = original.snapshot();
    let mut resumed = Idbd::try_from(snapshot.clone()).expect("valid snapshot");
    assert_same_state(&resumed.snapshot(), &snapshot);
    assert_ne!(
        resumed.log_step_sizes()[0],
        resumed.config().initial_beta(),
        "adapted step sizes are part of the state"
    );

    for _ in 0..2_000 {
        let (x, target) = example(&mut rng);
        let a = original.update(&x, target).expect("original update");
        let b = resumed.update(&x, target).expect("resumed update");
        assert_eq!(a.prediction.to_bits(), b.prediction.to_bits());
        assert_eq!(a.error.to_bits(), b.error.to_bits());
        assert_eq!(a.bounded_features, b.bounded_features);
    }
    assert_same_state(&resumed.snapshot(), &original.snapshot());
}

#[test]
fn c21_weights_only_restoration_is_not_equivalent() {
    let mut rng = TestRng::new(211);
    let mut original = Idbd::new(5, config(0.05, 0.02)).expect("valid learner");
    for _ in 0..2_000 {
        let (x, target) = example(&mut rng);
        original.update(&x, target).expect("update");
    }
    let mut weights_only = Idbd::from_state(State {
        weights: original.weights().to_vec(),
        ..Idbd::new(5, *original.config()).expect("fresh").snapshot()
    })
    .expect("valid state");
    let (x, target) = example(&mut rng);
    original.update(&x, target).expect("update");
    weights_only.update(&x, target).expect("update");
    assert_ne!(original.weights(), weights_only.weights());
}

fn valid_state(config: Config) -> State {
    State {
        config,
        weights: vec![0.5, -0.25],
        betas: vec![-3.0, -2.0],
        traces: vec![0.1, 0.0],
    }
}

#[test]
fn c22_reject_an_invalid_snapshot() {
    let base = valid_state(config(0.05, 0.01));
    let cases = [
        (
            "empty state vectors",
            State {
                weights: vec![],
                betas: vec![],
                traces: vec![],
                ..base.clone()
            },
            StateError::Empty,
        ),
        (
            "state vectors of different lengths",
            State {
                traces: vec![0.0],
                ..base.clone()
            },
            StateError::LengthMismatch {
                weights: 2,
                betas: 2,
                traces: 1,
            },
        ),
        (
            "a non-finite weight",
            State {
                weights: vec![0.5, f64::NAN],
                ..base.clone()
            },
            StateError::NonFiniteWeight { index: 1 },
        ),
        (
            "a non-finite beta",
            State {
                betas: vec![f64::NEG_INFINITY, -2.0],
                ..base.clone()
            },
            StateError::NonFiniteBeta { index: 0 },
        ),
        (
            "a non-finite trace",
            State {
                traces: vec![0.1, f64::INFINITY],
                ..base.clone()
            },
            StateError::NonFiniteTrace { index: 1 },
        ),
        (
            "beta outside its configured bounds",
            State {
                betas: vec![-3.0, -11.0],
                ..valid_state(safety_config(0.05, 0.01))
            },
            StateError::BetaOutOfBounds {
                index: 1,
                beta: -11.0,
            },
        ),
        (
            "beta whose exponential is zero",
            State {
                betas: vec![-800.0, -2.0],
                ..base.clone()
            },
            StateError::UnrepresentableStepSize {
                index: 0,
                beta: -800.0,
            },
        ),
        (
            "beta whose exponential is infinite",
            State {
                betas: vec![-3.0, 800.0],
                ..base.clone()
            },
            StateError::UnrepresentableStepSize {
                index: 1,
                beta: 800.0,
            },
        ),
    ];
    assert!(Idbd::from_state(base).is_ok(), "base state is valid");
    for (fault, state, expected) in cases {
        match Idbd::from_state(state) {
            Err(error) => assert_eq!(error, expected, "{fault}"),
            Ok(learner) => panic!("{fault}: produced {learner:?}"),
        }
    }
}

#[test]
fn c22_invalid_configuration_cannot_enter_a_snapshot() {
    // `State::config` has type `Config`, whose constructors validate, so a snapshot
    // with an invalid configuration cannot be built in safe code.
    assert!(Config::new(0.05, f64::NAN).is_err(), "invalid theta");
    assert!(
        Config::new(-1.0, 0.01).is_err(),
        "invalid initial step size"
    );
}
