//! Validated learner configuration and optional beta bounds.

use std::error;
use std::fmt;

/// Optional limits on each coordinate's log step size, beta.
///
/// Sutton (1992) suggests bounding beta from below to prevent arithmetic underflow and
/// limiting the change in beta on one step. These limits are safety precautions. They do
/// not guarantee convergence, and Sutton's reported experiments ran without them.
///
/// On every update the learner first computes and validates the raw increment
/// `theta * error * x_i * h_i` and the unbounded proposal `beta_i + increment`. Then it
/// clips the increment to `[-max_beta_increment, max_beta_increment]`, adds it to the old
/// beta and applies the floor and ceiling, in that order.
///
/// # Examples
///
/// ```
/// use idbd::Bounds;
///
/// # fn main() -> Result<(), idbd::ConfigError> {
/// let custom = Bounds::none().with_min_beta(-8.0)?.with_max_beta(0.0)?;
/// assert_eq!(custom.min_beta(), Some(-8.0));
/// assert_eq!(Bounds::sutton_safety().max_beta_increment(), Some(2.0));
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Bounds {
    min_beta: Option<f64>,
    max_beta: Option<f64>,
    max_beta_increment: Option<f64>,
}

/// Beta floor of the optional safety configuration, from Sutton (1992, p. 173).
const SAFETY_MIN_BETA: f64 = -10.0;

/// Largest absolute beta change per update in the optional safety configuration, from
/// Sutton (1992, p. 173).
const SAFETY_MAX_BETA_INCREMENT: f64 = 2.0;

impl Bounds {
    /// Returns bounds that leave the IDBD equations unchanged.
    pub const fn none() -> Self {
        Self {
            min_beta: None,
            max_beta: None,
            max_beta_increment: None,
        }
    }

    /// Returns the safety bounds suggested by Sutton (1992): a beta floor of -10, an
    /// increment limit of 2 and no ceiling.
    ///
    /// Disable these bounds to reproduce the paper's experiments.
    pub const fn sutton_safety() -> Self {
        Self {
            min_beta: Some(SAFETY_MIN_BETA),
            max_beta: None,
            max_beta_increment: Some(SAFETY_MAX_BETA_INCREMENT),
        }
    }

    /// Sets the smallest allowed beta. The floor may equal the ceiling.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::InvalidBound`] if `min_beta` is not finite or if
    /// `exp(min_beta)` underflows to zero, and
    /// [`ConfigError::InvertedBetaBounds`] if it exceeds a configured ceiling.
    pub fn with_min_beta(self, min_beta: f64) -> Result<Self, ConfigError> {
        let min_beta = beta_bound(BoundKind::MinBeta, min_beta)?;
        check_order(Some(min_beta), self.max_beta)?;
        Ok(Self {
            min_beta: Some(min_beta),
            ..self
        })
    }

    /// Sets the largest allowed beta. The ceiling may equal the floor.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::InvalidBound`] if `max_beta` is not finite or if
    /// `exp(max_beta)` overflows, and
    /// [`ConfigError::InvertedBetaBounds`] if it is below a configured floor.
    pub fn with_max_beta(self, max_beta: f64) -> Result<Self, ConfigError> {
        let max_beta = beta_bound(BoundKind::MaxBeta, max_beta)?;
        check_order(self.min_beta, Some(max_beta))?;
        Ok(Self {
            max_beta: Some(max_beta),
            ..self
        })
    }

    /// Sets the largest absolute change in beta that one update can apply.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::InvalidBound`] unless `max_beta_increment` is finite and
    /// strictly positive.
    pub fn with_max_beta_increment(self, max_beta_increment: f64) -> Result<Self, ConfigError> {
        let max_beta_increment = finite_bound(BoundKind::MaxBetaIncrement, max_beta_increment)?;
        if max_beta_increment <= 0.0 {
            return Err(ConfigError::InvalidBound {
                bound: BoundKind::MaxBetaIncrement,
                value: max_beta_increment,
            });
        }
        Ok(Self {
            max_beta_increment: Some(max_beta_increment),
            ..self
        })
    }

    /// Returns the beta floor, if configured.
    pub const fn min_beta(&self) -> Option<f64> {
        self.min_beta
    }

    /// Returns the beta ceiling, if configured.
    pub const fn max_beta(&self) -> Option<f64> {
        self.max_beta
    }

    /// Returns the per-update limit on the absolute beta change, if configured.
    pub const fn max_beta_increment(&self) -> Option<f64> {
        self.max_beta_increment
    }

    /// Returns `true` if no bound is configured.
    pub const fn is_none(&self) -> bool {
        self.min_beta.is_none() && self.max_beta.is_none() && self.max_beta_increment.is_none()
    }

    /// Returns `true` if `beta` lies within the configured floor and ceiling.
    pub fn contains(&self, beta: f64) -> bool {
        self.min_beta.is_none_or(|min| beta >= min) && self.max_beta.is_none_or(|max| beta <= max)
    }

    /// Applies the bounds to a validated raw increment, in the documented order.
    pub(crate) fn apply(&self, old_beta: f64, raw_increment: f64) -> f64 {
        let increment = match self.max_beta_increment {
            Some(limit) => raw_increment.clamp(-limit, limit),
            None => raw_increment,
        };
        let mut beta = old_beta + increment;
        if let Some(min) = self.min_beta {
            beta = beta.max(min);
        }
        if let Some(max) = self.max_beta {
            beta = beta.min(max);
        }
        beta
    }
}

fn finite_bound(bound: BoundKind, value: f64) -> Result<f64, ConfigError> {
    if value.is_finite() {
        Ok(value)
    } else {
        Err(ConfigError::InvalidBound { bound, value })
    }
}

/// Accepts a floor or ceiling only if its step size `exp(beta)` is representable, so a
/// bound can never clamp beta onto an unusable step size.
fn beta_bound(bound: BoundKind, value: f64) -> Result<f64, ConfigError> {
    if is_valid_step_size(value.exp()) {
        Ok(value)
    } else {
        Err(ConfigError::InvalidBound { bound, value })
    }
}

fn check_order(min_beta: Option<f64>, max_beta: Option<f64>) -> Result<(), ConfigError> {
    match (min_beta, max_beta) {
        (Some(min_beta), Some(max_beta)) if min_beta > max_beta => {
            Err(ConfigError::InvertedBetaBounds { min_beta, max_beta })
        }
        _ => Ok(()),
    }
}

/// Validated settings for an [`Idbd`](crate::Idbd) learner.
///
/// A `Config` holds the initial step size `alpha_0`, the meta step size `theta` and
/// optional [`Bounds`]. Every learner coordinate starts with `beta = ln(alpha_0)`.
///
/// # Examples
///
/// ```
/// use idbd::{Bounds, Config};
///
/// # fn main() -> Result<(), idbd::ConfigError> {
/// let config = Config::new(0.05, 0.01)?.with_bounds(Bounds::sutton_safety())?;
/// assert_eq!(config.initial_step_size(), 0.05);
/// assert_eq!(config.meta_step_size(), 0.01);
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Config {
    initial_step_size: f64,
    meta_step_size: f64,
    initial_beta: f64,
    bounds: Bounds,
}

impl Config {
    /// Creates a configuration without bounds.
    ///
    /// `initial_step_size` is the starting learning rate of every input. `meta_step_size`
    /// is theta, the learning rate of the step sizes themselves. A theta of zero disables
    /// meta learning, and the learner then behaves as LMS with fixed per-input rates.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::InvalidInitialStepSize`] unless `initial_step_size` is
    /// finite, strictly positive, and survives the round trip through `ln` and `exp`
    /// without underflow. Returns [`ConfigError::InvalidMetaStepSize`] unless
    /// `meta_step_size` is finite and nonnegative.
    pub fn new(initial_step_size: f64, meta_step_size: f64) -> Result<Self, ConfigError> {
        let initial_beta = initial_step_size.ln();
        let representable = initial_step_size.is_finite()
            && initial_step_size > 0.0
            && is_valid_step_size(initial_beta.exp());
        if !representable {
            return Err(ConfigError::InvalidInitialStepSize {
                value: initial_step_size,
            });
        }
        if !(meta_step_size.is_finite() && meta_step_size >= 0.0) {
            return Err(ConfigError::InvalidMetaStepSize {
                value: meta_step_size,
            });
        }
        Ok(Self {
            initial_step_size,
            meta_step_size,
            initial_beta,
            bounds: Bounds::none(),
        })
    }

    /// Replaces the bounds.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::InitialBetaOutOfBounds`] if `ln(initial_step_size)` lies
    /// outside the floor and ceiling of `bounds`.
    pub fn with_bounds(self, bounds: Bounds) -> Result<Self, ConfigError> {
        if !bounds.contains(self.initial_beta) {
            return Err(ConfigError::InitialBetaOutOfBounds {
                initial_beta: self.initial_beta,
            });
        }
        Ok(Self { bounds, ..self })
    }

    /// Returns the initial step size, `alpha_0`.
    pub const fn initial_step_size(&self) -> f64 {
        self.initial_step_size
    }

    /// Returns the meta step size, theta.
    pub const fn meta_step_size(&self) -> f64 {
        self.meta_step_size
    }

    /// Returns `ln(initial_step_size)`, the beta of a newly constructed learner.
    pub const fn initial_beta(&self) -> f64 {
        self.initial_beta
    }

    /// Returns the configured bounds.
    pub const fn bounds(&self) -> &Bounds {
        &self.bounds
    }
}

/// Returns `true` if `alpha` is a usable step size: finite and strictly positive.
pub(crate) fn is_valid_step_size(alpha: f64) -> bool {
    alpha.is_finite() && alpha > 0.0
}

/// Identifies one of the optional [`Bounds`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum BoundKind {
    /// The beta floor.
    MinBeta,
    /// The beta ceiling.
    MaxBeta,
    /// The per-update limit on the absolute beta change.
    MaxBetaIncrement,
}

impl fmt::Display for BoundKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::MinBeta => "minimum beta",
            Self::MaxBeta => "maximum beta",
            Self::MaxBetaIncrement => "maximum beta increment",
        })
    }
}

/// An invalid learner configuration.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum ConfigError {
    /// The learner needs at least one feature.
    ZeroFeatures,
    /// The initial step size is not finite and strictly positive, or its logarithm
    /// does not map back to a positive finite step size.
    InvalidInitialStepSize {
        /// The rejected value.
        value: f64,
    },
    /// The meta step size is negative or not finite.
    InvalidMetaStepSize {
        /// The rejected value.
        value: f64,
    },
    /// A bound is not finite, a beta bound's exponential is zero or infinite, or the
    /// increment limit is not strictly positive.
    InvalidBound {
        /// The rejected bound.
        bound: BoundKind,
        /// The rejected value.
        value: f64,
    },
    /// The beta floor exceeds the beta ceiling.
    InvertedBetaBounds {
        /// The configured floor.
        min_beta: f64,
        /// The configured ceiling.
        max_beta: f64,
    },
    /// `ln(initial_step_size)` lies outside the configured beta floor and ceiling.
    InitialBetaOutOfBounds {
        /// The initial beta.
        initial_beta: f64,
    },
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroFeatures => f.write_str("a learner needs at least one feature"),
            Self::InvalidInitialStepSize { value } => write!(
                f,
                "initial step size {value} is not a finite, strictly positive, representable value"
            ),
            Self::InvalidMetaStepSize { value } => {
                write!(f, "meta step size {value} is not finite and nonnegative")
            }
            Self::InvalidBound { bound, value } => write!(f, "{bound} {value} is invalid"),
            Self::InvertedBetaBounds { min_beta, max_beta } => {
                write!(f, "minimum beta {min_beta} exceeds maximum beta {max_beta}")
            }
            Self::InitialBetaOutOfBounds { initial_beta } => write!(
                f,
                "initial beta {initial_beta} lies outside the configured beta bounds"
            ),
        }
    }
}

impl error::Error for ConfigError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[expect(clippy::float_cmp, reason = "clipping must produce exact values")]
    fn apply_clips_increment_before_floor_and_ceiling() {
        let bounds = Bounds::none()
            .with_min_beta(-10.0)
            .and_then(|b| b.with_max_beta(-2.0))
            .and_then(|b| b.with_max_beta_increment(2.0))
            .expect("valid bounds");
        assert_eq!(bounds.apply(-5.0, 3.0), -3.0);
        assert_eq!(bounds.apply(-3.0, 3.0), -2.0);
        assert_eq!(bounds.apply(-9.0, -3.0), -10.0);
        assert_eq!(bounds.apply(-5.0, 0.5), -4.5);
    }

    #[test]
    #[expect(
        clippy::float_cmp,
        reason = "no bounds must add the raw increment exactly"
    )]
    fn none_applies_raw_increment() {
        assert_eq!(Bounds::none().apply(-5.0, 3.0), -2.0);
        assert!(Bounds::none().is_none());
        assert!(!Bounds::sutton_safety().is_none());
    }

    #[test]
    fn each_bound_is_stored_and_reported_independently() {
        let floor = Bounds::none().with_min_beta(-4.0).expect("valid floor");
        let ceiling = Bounds::none().with_max_beta(-1.5).expect("valid ceiling");
        let limit = Bounds::none()
            .with_max_beta_increment(0.25)
            .expect("valid limit");
        for (bounds, min, max, increment) in [
            (floor, Some(-4.0), None, None),
            (ceiling, None, Some(-1.5), None),
            (limit, None, None, Some(0.25)),
        ] {
            assert_eq!(bounds.min_beta(), min, "{bounds:?}");
            assert_eq!(bounds.max_beta(), max, "{bounds:?}");
            assert_eq!(bounds.max_beta_increment(), increment, "{bounds:?}");
            assert!(!bounds.is_none(), "{bounds:?}");
        }
    }

    #[test]
    fn contains_includes_endpoints() {
        let bounds = Bounds::none()
            .with_min_beta(-1.0)
            .and_then(|b| b.with_max_beta(-1.0))
            .expect("equal endpoints are valid");
        assert!(bounds.contains(-1.0));
        assert!(!bounds.contains(-1.000_000_1));
        assert!(!bounds.contains(-0.999_999_9));
    }

    #[test]
    fn errors_display_lowercase_messages() {
        let message = ConfigError::InvertedBetaBounds {
            min_beta: 1.0,
            max_beta: 0.0,
        }
        .to_string();
        assert_eq!(message, "minimum beta 1 exceeds maximum beta 0");
        assert_eq!(
            ConfigError::InvalidBound {
                bound: BoundKind::MaxBetaIncrement,
                value: 0.0
            }
            .to_string(),
            "maximum beta increment 0 is invalid"
        );
    }
}
