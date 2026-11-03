//! Complete learner snapshots and their validation.

use std::error;
use std::fmt;

use crate::config::{Config, is_valid_step_size};

/// The complete learning state of an [`Idbd`](crate::Idbd) learner.
///
/// A state holds everything needed to continue learning: the configuration and, per
/// feature, the weight, the log step size beta and the adaptation trace. Create one with
/// [`Idbd::snapshot`](crate::Idbd::snapshot) and restore it with
/// [`Idbd::from_state`](crate::Idbd::from_state), which validates every field.
///
/// # Examples
///
/// ```
/// use idbd::{Config, Idbd};
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let mut learner = Idbd::new(2, Config::new(0.1, 0.01)?)?;
/// learner.update(&[1.0, 2.0], 1.0)?;
///
/// let resumed = Idbd::from_state(learner.snapshot())?;
/// assert_eq!(resumed.weights(), learner.weights());
/// assert_eq!(resumed.log_step_sizes(), learner.log_step_sizes());
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct State {
    /// The learner configuration.
    pub config: Config,
    /// One weight per feature.
    pub weights: Vec<f64>,
    /// One log step size per feature; `alpha_i = exp(betas[i])`.
    pub betas: Vec<f64>,
    /// One adaptation trace per feature.
    pub traces: Vec<f64>,
}

impl State {
    /// Assembles a state from its parts without validating them.
    ///
    /// [`Idbd::from_state`](crate::Idbd::from_state) validates the state when it
    /// restores a learner.
    pub fn new(config: Config, weights: Vec<f64>, betas: Vec<f64>, traces: Vec<f64>) -> Self {
        Self {
            config,
            weights,
            betas,
            traces,
        }
    }

    pub(crate) fn validate(&self) -> Result<(), StateError> {
        let (weights, betas, traces) = (self.weights.len(), self.betas.len(), self.traces.len());
        if weights != betas || weights != traces {
            return Err(StateError::LengthMismatch {
                weights,
                betas,
                traces,
            });
        }
        if weights == 0 {
            return Err(StateError::Empty);
        }
        let bounds = self.config.bounds();
        let rows = self.weights.iter().zip(&self.betas).zip(&self.traces);
        for (index, ((&weight, &beta), &trace)) in rows.enumerate() {
            if !weight.is_finite() {
                return Err(StateError::NonFiniteWeight { index });
            }
            if !beta.is_finite() {
                return Err(StateError::NonFiniteBeta { index });
            }
            if !trace.is_finite() {
                return Err(StateError::NonFiniteTrace { index });
            }
            if !bounds.contains(beta) {
                return Err(StateError::BetaOutOfBounds { index, beta });
            }
            if !is_valid_step_size(beta.exp()) {
                return Err(StateError::UnrepresentableStepSize { index, beta });
            }
        }
        Ok(())
    }
}

/// A [`State`] that cannot be restored.
///
/// The configuration inside a state is always valid, because [`Config`] can only be
/// built through its validating constructors.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum StateError {
    /// The state vectors are empty.
    Empty,
    /// The state vectors differ in length.
    LengthMismatch {
        /// Number of weights.
        weights: usize,
        /// Number of betas.
        betas: usize,
        /// Number of traces.
        traces: usize,
    },
    /// A weight is NaN or infinite.
    NonFiniteWeight {
        /// The feature index.
        index: usize,
    },
    /// A beta is NaN or infinite.
    NonFiniteBeta {
        /// The feature index.
        index: usize,
    },
    /// A trace is NaN or infinite.
    NonFiniteTrace {
        /// The feature index.
        index: usize,
    },
    /// A beta lies outside the configured floor and ceiling.
    BetaOutOfBounds {
        /// The feature index.
        index: usize,
        /// The rejected beta.
        beta: f64,
    },
    /// `exp(beta)` underflows to zero or overflows to infinity.
    UnrepresentableStepSize {
        /// The feature index.
        index: usize,
        /// The rejected beta.
        beta: f64,
    },
}

impl fmt::Display for StateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("state has no features"),
            Self::LengthMismatch {
                weights,
                betas,
                traces,
            } => write!(
                f,
                "state vectors differ in length: {weights} weights, {betas} betas, {traces} traces"
            ),
            Self::NonFiniteWeight { index } => write!(f, "weight {index} is not finite"),
            Self::NonFiniteBeta { index } => write!(f, "beta {index} is not finite"),
            Self::NonFiniteTrace { index } => write!(f, "trace {index} is not finite"),
            Self::BetaOutOfBounds { index, beta } => {
                write!(
                    f,
                    "beta {index} ({beta}) lies outside the configured bounds"
                )
            }
            Self::UnrepresentableStepSize { index, beta } => write!(
                f,
                "beta {index} ({beta}) does not produce a finite, strictly positive step size"
            ),
        }
    }
}

impl error::Error for StateError {}
