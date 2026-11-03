//! Rule: Construction establishes a valid and explicit learning state.

use idbd::{BoundKind, Bounds, Config, ConfigError, Idbd};

use crate::support::{assert_close, config};

#[test]
fn c01_initialize_a_learner() {
    let config = config(0.05, 0.01);
    let learner = Idbd::new(3, config).expect("valid learner");

    assert_eq!(learner.weights(), &[0.0; 3]);
    assert_eq!(learner.traces(), &[0.0; 3]);
    for &beta in learner.log_step_sizes() {
        assert_close(beta, 0.05_f64.ln(), "beta");
    }
    assert_eq!(learner.step_sizes().len(), 3);
    for alpha in learner.step_sizes() {
        assert!(alpha.is_finite() && alpha > 0.0, "alpha {alpha}");
    }
    assert_eq!(learner.num_features(), 3);
    assert_eq!(learner.config(), &config);
    assert_eq!(learner.config().initial_step_size(), 0.05);
    assert_eq!(learner.config().meta_step_size(), 0.01);
    assert!(learner.config().bounds().is_none());
}

fn with_bounds(bounds: Result<Bounds, ConfigError>) -> Result<Idbd, ConfigError> {
    let config = Config::new(0.05, 0.01)?.with_bounds(bounds?)?;
    Idbd::new(3, config)
}

fn with_step_sizes(initial_step_size: f64, meta_step_size: f64) -> Result<Idbd, ConfigError> {
    Idbd::new(3, Config::new(initial_step_size, meta_step_size)?)
}

#[test]
#[expect(
    clippy::too_many_lines,
    reason = "one table row per fault in the specification"
)]
fn c02_reject_invalid_configuration() {
    let none = Bounds::none();
    let cases: Vec<(&str, Result<Idbd, ConfigError>, ConfigError)> = vec![
        (
            "zero features",
            Idbd::new(0, config(0.05, 0.01)),
            ConfigError::ZeroFeatures,
        ),
        (
            "initial alpha is zero",
            with_step_sizes(0.0, 0.01),
            ConfigError::InvalidInitialStepSize { value: 0.0 },
        ),
        (
            "initial alpha is negative",
            with_step_sizes(-0.05, 0.01),
            ConfigError::InvalidInitialStepSize { value: -0.05 },
        ),
        (
            "initial alpha is positive infinity",
            with_step_sizes(f64::INFINITY, 0.01),
            ConfigError::InvalidInitialStepSize {
                value: f64::INFINITY,
            },
        ),
        (
            "initial alpha is negative infinity",
            with_step_sizes(f64::NEG_INFINITY, 0.01),
            ConfigError::InvalidInitialStepSize {
                value: f64::NEG_INFINITY,
            },
        ),
        (
            "theta is negative",
            with_step_sizes(0.05, -0.01),
            ConfigError::InvalidMetaStepSize { value: -0.01 },
        ),
        (
            "theta is positive infinity",
            with_step_sizes(0.05, f64::INFINITY),
            ConfigError::InvalidMetaStepSize {
                value: f64::INFINITY,
            },
        ),
        (
            "theta is negative infinity",
            with_step_sizes(0.05, f64::NEG_INFINITY),
            ConfigError::InvalidMetaStepSize {
                value: f64::NEG_INFINITY,
            },
        ),
        (
            "a configured bound is non-finite (maximum beta)",
            with_bounds(none.with_max_beta(f64::INFINITY)),
            ConfigError::InvalidBound {
                bound: BoundKind::MaxBeta,
                value: f64::INFINITY,
            },
        ),
        (
            "a configured bound is non-finite (increment)",
            with_bounds(none.with_max_beta_increment(f64::NEG_INFINITY)),
            ConfigError::InvalidBound {
                bound: BoundKind::MaxBetaIncrement,
                value: f64::NEG_INFINITY,
            },
        ),
        (
            "minimum beta whose step size underflows",
            with_bounds(none.with_min_beta(-746.0)),
            ConfigError::InvalidBound {
                bound: BoundKind::MinBeta,
                value: -746.0,
            },
        ),
        (
            "maximum beta whose step size overflows",
            with_bounds(none.with_max_beta(710.0)),
            ConfigError::InvalidBound {
                bound: BoundKind::MaxBeta,
                value: 710.0,
            },
        ),
        (
            "maximum beta increment is zero",
            with_bounds(none.with_max_beta_increment(0.0)),
            ConfigError::InvalidBound {
                bound: BoundKind::MaxBetaIncrement,
                value: 0.0,
            },
        ),
        (
            "maximum beta increment is negative",
            with_bounds(none.with_max_beta_increment(-2.0)),
            ConfigError::InvalidBound {
                bound: BoundKind::MaxBetaIncrement,
                value: -2.0,
            },
        ),
        (
            "minimum beta exceeds maximum beta",
            with_bounds(none.with_min_beta(0.0).and_then(|b| b.with_max_beta(-1.0))),
            ConfigError::InvertedBetaBounds {
                min_beta: 0.0,
                max_beta: -1.0,
            },
        ),
        (
            "maximum beta below an existing minimum",
            with_bounds(none.with_max_beta(-1.0).and_then(|b| b.with_min_beta(0.0))),
            ConfigError::InvertedBetaBounds {
                min_beta: 0.0,
                max_beta: -1.0,
            },
        ),
        (
            "initial log alpha is below minimum beta",
            with_bounds(none.with_min_beta(-1.0)),
            ConfigError::InitialBetaOutOfBounds {
                initial_beta: 0.05_f64.ln(),
            },
        ),
        (
            "initial log alpha is above maximum beta",
            with_bounds(none.with_max_beta(-5.0)),
            ConfigError::InitialBetaOutOfBounds {
                initial_beta: 0.05_f64.ln(),
            },
        ),
    ];
    for (fault, result, expected) in cases {
        match result {
            Err(error) => assert_eq!(error, expected, "{fault}"),
            Ok(learner) => panic!("{fault}: produced {learner:?}"),
        }
    }
}

#[test]
fn c02_reject_nan_configuration() {
    // NaN payloads never compare equal, so these rows match on the variant.
    assert!(matches!(
        with_step_sizes(f64::NAN, 0.01),
        Err(ConfigError::InvalidInitialStepSize { value }) if value.is_nan()
    ));
    assert!(matches!(
        with_step_sizes(0.05, f64::NAN),
        Err(ConfigError::InvalidMetaStepSize { value }) if value.is_nan()
    ));
    assert!(matches!(
        with_bounds(Bounds::none().with_min_beta(f64::NAN)),
        Err(ConfigError::InvalidBound { bound: BoundKind::MinBeta, value }) if value.is_nan()
    ));
}

#[test]
fn c02_accept_boundary_configuration() {
    let equal = Bounds::none()
        .with_min_beta(0.05_f64.ln())
        .and_then(|b| b.with_max_beta(0.05_f64.ln()));
    assert!(
        with_bounds(equal).is_ok(),
        "equal endpoints at the initial beta"
    );
    assert!(
        with_step_sizes(0.05, 0.0).is_ok(),
        "theta zero disables meta learning"
    );
    assert!(
        with_step_sizes(f64::MIN_POSITIVE, 0.01).is_ok(),
        "smallest normal step size"
    );
}
