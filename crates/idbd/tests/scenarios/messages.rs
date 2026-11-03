//! Error messages are part of the public behavior: lowercase, no trailing punctuation,
//! and they name the failing value, coordinate or stage.

use idbd::autostep::{self, Autostep};
use idbd::{BoundKind, ConfigError, Error, Stage, StateError};

#[track_caller]
fn assert_message(error: &dyn std::error::Error, expected: &str) {
    let message = error.to_string();
    assert_eq!(message, expected, "display text");
    assert!(
        message.starts_with(|c: char| c.is_lowercase() || c.is_ascii_digit()),
        "{message}"
    );
    assert!(!message.ends_with('.'), "{message}");
    assert!(error.source().is_none(), "{message}");
}

#[test]
fn config_error_messages() {
    let cases = [
        (
            ConfigError::ZeroFeatures,
            "a learner needs at least one feature",
        ),
        (
            ConfigError::InvalidInitialStepSize { value: -1.0 },
            "initial step size -1 is not a finite, strictly positive, representable value",
        ),
        (
            ConfigError::InvalidMetaStepSize { value: -0.5 },
            "meta step size -0.5 is not finite and nonnegative",
        ),
        (
            ConfigError::InvalidBound {
                bound: BoundKind::MinBeta,
                value: f64::NAN,
            },
            "minimum beta NaN is invalid",
        ),
        (
            ConfigError::InvalidBound {
                bound: BoundKind::MaxBeta,
                value: f64::INFINITY,
            },
            "maximum beta inf is invalid",
        ),
        (
            ConfigError::InvertedBetaBounds {
                min_beta: 1.0,
                max_beta: 0.5,
            },
            "minimum beta 1 exceeds maximum beta 0.5",
        ),
        (
            ConfigError::InitialBetaOutOfBounds { initial_beta: -3.0 },
            "initial beta -3 lies outside the configured beta bounds",
        ),
        (
            ConfigError::InvalidTimeScale { value: 0.5 },
            "time scale 0.5 is not finite and at least 1",
        ),
    ];
    for (error, expected) in cases {
        assert_message(&error, expected);
    }
}

#[test]
fn state_error_messages() {
    let cases = [
        (StateError::Empty, "state has no features"),
        (
            StateError::LengthMismatch {
                weights: 3,
                betas: 3,
                traces: 2,
            },
            "state vectors differ in length: 3 weights, 3 betas, 2 traces",
        ),
        (
            StateError::NonFiniteWeight { index: 1 },
            "weight 1 is not finite",
        ),
        (
            StateError::NonFiniteBeta { index: 2 },
            "beta 2 is not finite",
        ),
        (
            StateError::NonFiniteTrace { index: 0 },
            "trace 0 is not finite",
        ),
        (
            StateError::BetaOutOfBounds {
                index: 4,
                beta: -11.0,
            },
            "beta 4 (-11) lies outside the configured bounds",
        ),
        (
            StateError::UnrepresentableStepSize {
                index: 5,
                beta: 800.0,
            },
            "beta 5 (800) does not produce a finite, strictly positive step size",
        ),
    ];
    for (error, expected) in cases {
        assert_message(&error, expected);
    }
}

#[test]
fn operation_error_messages() {
    let numerical = |stage, coordinate| Error::Numerical { stage, coordinate };
    let cases = [
        (
            Error::DimensionMismatch {
                expected: 3,
                found: 4,
            },
            "expected 3 features, found 4",
        ),
        (
            Error::NonFiniteFeature { index: 2 },
            "feature 2 is not finite",
        ),
        (Error::NonFiniteTarget, "target is not finite"),
        (
            numerical(Stage::Prediction, None),
            "numerical failure at the prediction stage",
        ),
        (
            numerical(Stage::Residual, None),
            "numerical failure at the residual stage",
        ),
        (
            numerical(Stage::MetaStep, Some(0)),
            "numerical failure at the meta step stage for coordinate 0",
        ),
        (
            numerical(Stage::StepSize, Some(1)),
            "numerical failure at the step size stage for coordinate 1",
        ),
        (
            numerical(Stage::Weight, Some(2)),
            "numerical failure at the weight stage for coordinate 2",
        ),
        (
            numerical(Stage::Trace, Some(3)),
            "numerical failure at the trace stage for coordinate 3",
        ),
        (
            numerical(Stage::Normalizer, Some(4)),
            "numerical failure at the normalizer stage for coordinate 4",
        ),
        (
            numerical(Stage::EffectiveStepSize, None),
            "numerical failure at the effective step size stage",
        ),
    ];
    for (error, expected) in cases {
        assert_message(&error, expected);
    }
}

#[test]
fn autostep_state_error_messages() {
    let cases = [
        (autostep::StateError::Empty, "state has no features"),
        (
            autostep::StateError::LengthMismatch {
                weights: 3,
                step_sizes: 3,
                traces: 3,
                normalizers: 2,
            },
            "state vectors differ in length: 3 weights, 3 step sizes, 3 traces, 2 normalizers",
        ),
        (
            autostep::StateError::NonFiniteWeight { index: 1 },
            "weight 1 is not finite",
        ),
        (
            autostep::StateError::InvalidStepSize {
                index: 2,
                step_size: 0.0,
            },
            "step size 2 (0) is not finite and strictly positive",
        ),
        (
            autostep::StateError::NonFiniteTrace { index: 0 },
            "trace 0 is not finite",
        ),
        (
            autostep::StateError::InvalidNormalizer {
                index: 4,
                normalizer: -1.0,
            },
            "normalizer 4 (-1) is not finite and nonnegative",
        ),
    ];
    for (error, expected) in cases {
        assert_message(&error, expected);
    }
}

#[test]
fn autostep_debug_shows_configuration_and_every_vector() {
    let learner = Autostep::new(1, autostep::Config::new(0.25).expect("valid")).expect("valid");
    assert_eq!(
        format!("{learner:?}"),
        "Autostep { config: Config { initial_step_size: 0.25, meta_step_size: 0.01, \
         time_scale: 10000.0 }, weights: [0.0], step_sizes: [0.25], traces: [0.0], \
         normalizers: [0.0], .. }"
    );
}
