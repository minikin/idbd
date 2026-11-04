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
    let mut fresh = Idbd::new(5, *original.config()).expect("fresh").snapshot();
    fresh.weights = original.weights().to_vec();
    let mut weights_only = Idbd::from_state(fresh).expect("valid state");
    let (x, target) = example(&mut rng);
    original.update(&x, target).expect("update");
    weights_only.update(&x, target).expect("update");
    assert_ne!(original.weights(), weights_only.weights());
}

fn valid_state(config: Config) -> State {
    State::new(config, vec![0.5, -0.25])
        .with_betas(vec![-3.0, -2.0])
        .with_traces(vec![0.1, 0.0])
}

fn with(mut state: State, change: impl FnOnce(&mut State)) -> State {
    change(&mut state);
    state
}

#[test]
fn c22_reject_an_invalid_snapshot() {
    let base = valid_state(config(0.05, 0.01));
    let cases = [
        (
            "empty state vectors",
            with(base.clone(), |s| {
                s.weights = vec![];
                s.betas = vec![];
                s.traces = vec![];
            }),
            StateError::Empty,
        ),
        (
            "state vectors of different lengths",
            with(base.clone(), |s| {
                s.traces = vec![0.0];
            }),
            StateError::LengthMismatch {
                weights: 2,
                betas: 2,
                traces: 1,
            },
        ),
        (
            "a non-finite weight",
            with(base.clone(), |s| {
                s.weights = vec![0.5, f64::NAN];
            }),
            StateError::NonFiniteWeight { index: 1 },
        ),
        (
            "a non-finite beta",
            with(base.clone(), |s| {
                s.betas = vec![f64::NEG_INFINITY, -2.0];
            }),
            StateError::NonFiniteBeta { index: 0 },
        ),
        (
            "a non-finite trace",
            with(base.clone(), |s| {
                s.traces = vec![0.1, f64::INFINITY];
            }),
            StateError::NonFiniteTrace { index: 1 },
        ),
        (
            "beta outside its configured bounds",
            with(valid_state(safety_config(0.05, 0.01)), |s| {
                s.betas = vec![-3.0, -11.0];
            }),
            StateError::BetaOutOfBounds {
                index: 1,
                beta: -11.0,
            },
        ),
        (
            "beta whose exponential is zero",
            with(base.clone(), |s| {
                s.betas = vec![-800.0, -2.0];
            }),
            StateError::UnrepresentableStepSize {
                index: 0,
                beta: -800.0,
            },
        ),
        (
            "beta whose exponential is infinite",
            with(base.clone(), |s| {
                s.betas = vec![-3.0, 800.0];
            }),
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

#[test]
fn c21_clone_from_copies_a_learner_of_any_size() {
    let mut source = Idbd::new(3, safety_config(0.2, 0.05)).expect("valid");
    source.update(&[1.0, -0.5, 2.0], 0.7).expect("finite");
    let mut target = Idbd::new(5, config(0.1, 0.01)).expect("valid");
    target.clone_from(&source);
    assert_same_state(&target.snapshot(), &source.snapshot());
    let a = source.update(&[0.3, 0.1, -1.0], 0.2).expect("finite");
    let b = target
        .update(&[0.3, 0.1, -1.0], 0.2)
        .expect("the candidate buffer was resized");
    assert_eq!(a, b);
    assert_same_state(&target.snapshot(), &source.snapshot());
}

#[test]
fn c21_a_hand_built_state_names_every_vector() {
    let config = config(0.05, 0.01);
    let fresh = Idbd::new(2, config).expect("valid").snapshot();
    assert_eq!(State::new(config, vec![0.0, 0.0]), fresh);

    let state = valid_state(config);
    assert_eq!(state.weights, [0.5, -0.25]);
    assert_eq!(state.betas, [-3.0, -2.0]);
    assert_eq!(state.traces, [0.1, 0.0]);
    let learner = Idbd::from_state(state).expect("valid");
    assert_eq!(learner.log_step_sizes(), &[-3.0, -2.0]);
}
