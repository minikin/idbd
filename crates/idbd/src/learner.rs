//! The IDBD learner, its update report and its operation errors.

use std::error;
use std::fmt;
use std::iter::FusedIterator;
use std::mem;
use std::slice;

use crate::config::{Config, ConfigError, is_valid_step_size};
use crate::state::{State, StateError};

/// A linear online learner that adapts one step size per input with IDBD.
///
/// The learner predicts `w · x` and, for each example, applies Sutton's (1992)
/// recurrence:
///
/// ```text
/// prediction = sum_i(w_i * x_i)
/// error      = target - prediction
/// beta_i     = beta_i + theta * error * x_i * h_i       (old trace)
/// alpha_i    = exp(beta_i)                               (new step size)
/// w_i        = w_i + alpha_i * error * x_i
/// h_i        = h_i * max(0, 1 - alpha_i * x_i^2) + alpha_i * error * x_i
/// ```
///
/// Every coordinate uses the same pre-update error. Weights and traces start at zero and
/// every beta starts at `ln(initial_step_size)`.
///
/// # Atomic updates
///
/// [`update`](Self::update) computes the whole next state in buffers allocated at
/// construction, validates it, and commits it only if every value is finite and every
/// step size is strictly positive. A failed update leaves the configuration, weights,
/// betas and traces bit-for-bit unchanged. Prediction, update and inspection do not
/// allocate.
///
/// # Examples
///
/// ```
/// use idbd::{Config, Idbd};
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let mut learner = Idbd::new(2, Config::new(0.1, 0.01)?)?;
/// let report = learner.update(&[1.0, 2.0], 1.0)?;
/// assert_eq!(report.prediction, 0.0);
/// assert_eq!(report.error, 1.0);
/// assert!((learner.weights()[1] - 0.2).abs() < 1e-12);
/// # Ok(())
/// # }
/// ```
#[derive(Clone)]
pub struct Idbd {
    config: Config,
    weights: Vec<f64>,
    betas: Vec<f64>,
    traces: Vec<f64>,
    candidate: Candidate,
}

/// Preallocated storage for the next state, so failed updates never touch committed
/// state and successful ones do not allocate.
#[derive(Clone)]
struct Candidate {
    weights: Vec<f64>,
    betas: Vec<f64>,
    traces: Vec<f64>,
}

impl Candidate {
    fn new(num_features: usize) -> Self {
        Self {
            weights: vec![0.0; num_features],
            betas: vec![0.0; num_features],
            traces: vec![0.0; num_features],
        }
    }
}

impl Idbd {
    /// Creates a learner for `num_features` dense inputs.
    ///
    /// Add a constant input of `1.0` to learn an intercept. Construction allocates the
    /// committed state and an equally sized candidate buffer.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::ZeroFeatures`] if `num_features` is zero.
    pub fn new(num_features: usize, config: Config) -> Result<Self, ConfigError> {
        if num_features == 0 {
            return Err(ConfigError::ZeroFeatures);
        }
        Ok(Self {
            weights: vec![0.0; num_features],
            betas: vec![config.initial_beta(); num_features],
            traces: vec![0.0; num_features],
            candidate: Candidate::new(num_features),
            config,
        })
    }

    /// Restores a learner from a complete state, validating every value.
    ///
    /// Restoring the configuration, weights, betas and traces continues the same
    /// learning process. Restoring only weights would reset the adapted step sizes and
    /// traces.
    ///
    /// # Errors
    ///
    /// Returns a [`StateError`] if the vectors are empty or differ in length, if any
    /// value is not finite, if a beta lies outside the configured bounds, or if a beta's
    /// exponential is zero or infinite.
    pub fn from_state(state: State) -> Result<Self, StateError> {
        state.validate()?;
        let State {
            config,
            weights,
            betas,
            traces,
        } = state;
        let candidate = Candidate::new(weights.len());
        Ok(Self {
            config,
            weights,
            betas,
            traces,
            candidate,
        })
    }

    /// Returns a copy of the complete learning state.
    ///
    /// This allocates three vectors. Pass the result to [`Idbd::from_state`] to resume.
    pub fn snapshot(&self) -> State {
        State {
            config: self.config,
            weights: self.weights.clone(),
            betas: self.betas.clone(),
            traces: self.traces.clone(),
        }
    }

    /// Returns the prediction `w · x` for `features` without learning.
    ///
    /// # Errors
    ///
    /// Returns [`Error::DimensionMismatch`] or [`Error::NonFiniteFeature`] for invalid
    /// features, and [`Error::Numerical`] at [`Stage::Prediction`] if the dot product
    /// is not finite.
    pub fn predict(&self, features: &[f64]) -> Result<f64, Error> {
        self.check_features(features)?;
        self.dot(features)
    }

    /// Learns from one example and reports the prediction made before learning.
    ///
    /// # Errors
    ///
    /// Returns an input error for invalid features or a non-finite target, and
    /// [`Error::Numerical`] if any intermediate or resulting value is not representable.
    /// On error the committed state is unchanged.
    pub fn update(&mut self, features: &[f64], target: f64) -> Result<Update, Error> {
        self.check_features(features)?;
        if !target.is_finite() {
            return Err(Error::NonFiniteTarget);
        }
        let prediction = self.dot(features)?;
        let error = target - prediction;
        if !error.is_finite() {
            return Err(Error::Numerical {
                stage: Stage::Residual,
                coordinate: None,
            });
        }
        let bounded_features = self.propose(features, error)?;
        mem::swap(&mut self.weights, &mut self.candidate.weights);
        mem::swap(&mut self.betas, &mut self.candidate.betas);
        mem::swap(&mut self.traces, &mut self.candidate.traces);
        Ok(Update {
            prediction,
            error,
            bounded_features,
        })
    }

    /// Writes the next state into the candidate buffers and returns how many
    /// coordinates the bounds changed.
    fn propose(&mut self, features: &[f64], error: f64) -> Result<usize, Error> {
        let theta = self.config.meta_step_size();
        let bounds = *self.config.bounds();
        let mut bounded_features = 0;
        for (i, &x) in features.iter().enumerate() {
            let (weight, beta, trace) = (self.weights[i], self.betas[i], self.traces[i]);
            // A zero input changes nothing mathematically. Skipping it also keeps an
            // overflowing `alpha * error` from turning `* 0.0` into NaN.
            if x == 0.0 {
                self.candidate.weights[i] = weight;
                self.candidate.betas[i] = beta;
                self.candidate.traces[i] = trace;
                continue;
            }
            let raw_increment = theta * error * x * trace;
            let proposal = beta + raw_increment;
            // Committed betas never exceed ln(f64::MAX), so the proposal is finite
            // exactly when the raw increment is.
            if !proposal.is_finite() {
                return Err(Error::numerical(Stage::MetaStep, i));
            }
            let new_beta = bounds.apply(beta, raw_increment);
            #[expect(
                clippy::float_cmp,
                reason = "the contract counts any floating-point difference from the proposal"
            )]
            let bounded = new_beta != proposal;
            if bounded {
                bounded_features += 1;
            }
            let alpha = new_beta.exp();
            if !is_valid_step_size(alpha) {
                return Err(Error::numerical(Stage::StepSize, i));
            }
            let step = alpha * error * x;
            let new_weight = weight + step;
            if !new_weight.is_finite() {
                return Err(Error::numerical(Stage::Weight, i));
            }
            let decay = (1.0 - alpha * x * x).max(0.0);
            let new_trace = trace * decay + step;
            if !new_trace.is_finite() {
                return Err(Error::numerical(Stage::Trace, i));
            }
            self.candidate.weights[i] = new_weight;
            self.candidate.betas[i] = new_beta;
            self.candidate.traces[i] = new_trace;
        }
        Ok(bounded_features)
    }

    fn check_features(&self, features: &[f64]) -> Result<(), Error> {
        if features.len() != self.weights.len() {
            return Err(Error::DimensionMismatch {
                expected: self.weights.len(),
                found: features.len(),
            });
        }
        match features.iter().position(|x| !x.is_finite()) {
            Some(index) => Err(Error::NonFiniteFeature { index }),
            None => Ok(()),
        }
    }

    fn dot(&self, features: &[f64]) -> Result<f64, Error> {
        let prediction = self
            .weights
            .iter()
            .zip(features)
            .fold(0.0, |sum, (w, x)| sum + w * x);
        if prediction.is_finite() {
            Ok(prediction)
        } else {
            Err(Error::Numerical {
                stage: Stage::Prediction,
                coordinate: None,
            })
        }
    }

    /// Returns the number of input features.
    pub fn num_features(&self) -> usize {
        self.weights.len()
    }

    /// Returns the configuration.
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Returns the committed weights.
    pub fn weights(&self) -> &[f64] {
        &self.weights
    }

    /// Returns the committed log step sizes, beta.
    pub fn log_step_sizes(&self) -> &[f64] {
        &self.betas
    }

    /// Returns the committed adaptation traces, h.
    pub fn traces(&self) -> &[f64] {
        &self.traces
    }

    /// Returns an iterator over the step sizes `alpha_i = exp(beta_i)`.
    ///
    /// The iterator computes each value on demand and does not allocate.
    pub fn step_sizes(&self) -> StepSizes<'_> {
        StepSizes {
            betas: self.betas.iter(),
        }
    }
}

impl fmt::Debug for Idbd {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Idbd")
            .field("config", &self.config)
            .field("weights", &self.weights)
            .field("betas", &self.betas)
            .field("traces", &self.traces)
            .finish_non_exhaustive()
    }
}

impl TryFrom<State> for Idbd {
    type Error = StateError;

    fn try_from(state: State) -> Result<Self, Self::Error> {
        Self::from_state(state)
    }
}

/// Iterator over a learner's step sizes, returned by [`Idbd::step_sizes`].
#[derive(Debug, Clone)]
pub struct StepSizes<'a> {
    betas: slice::Iter<'a, f64>,
}

impl Iterator for StepSizes<'_> {
    type Item = f64;

    fn next(&mut self) -> Option<f64> {
        self.betas.next().map(|beta| beta.exp())
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.betas.size_hint()
    }
}

impl DoubleEndedIterator for StepSizes<'_> {
    fn next_back(&mut self) -> Option<f64> {
        self.betas.next_back().map(|beta| beta.exp())
    }
}

impl ExactSizeIterator for StepSizes<'_> {}

impl FusedIterator for StepSizes<'_> {}

/// Report of a successful [`Idbd::update`].
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub struct Update {
    /// Prediction made before learning from the example.
    pub prediction: f64,
    /// `target - prediction`, measured before learning.
    pub error: f64,
    /// Number of coordinates whose final beta differs from the unbounded proposal
    /// because of the configured bounds. A beta that only reaches a bound counts zero.
    pub bounded_features: usize,
}

/// The computation stage at which a numerical failure occurred.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Stage {
    /// The dot product `w · x`.
    Prediction,
    /// The residual `target - prediction`.
    Residual,
    /// The raw beta increment `theta * error * x_i * h_i` or the unbounded proposal.
    MetaStep,
    /// The step size `exp(beta_i)` underflowed to zero or overflowed.
    StepSize,
    /// The new weight.
    Weight,
    /// The new trace.
    Trace,
}

impl fmt::Display for Stage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Prediction => "prediction",
            Self::Residual => "residual",
            Self::MetaStep => "meta step",
            Self::StepSize => "step size",
            Self::Weight => "weight",
            Self::Trace => "trace",
        })
    }
}

/// An invalid input or a numerical failure during prediction or update.
///
/// Every failed operation leaves the committed learner state unchanged.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Error {
    /// The feature vector has the wrong length.
    DimensionMismatch {
        /// The learner's feature count.
        expected: usize,
        /// The supplied length.
        found: usize,
    },
    /// A feature is NaN or infinite.
    NonFiniteFeature {
        /// Index of the first non-finite feature.
        index: usize,
    },
    /// The target is NaN or infinite.
    NonFiniteTarget,
    /// A computed value was not finite, or a step size was not strictly positive.
    Numerical {
        /// The failed stage.
        stage: Stage,
        /// The failed coordinate, for per-coordinate stages.
        coordinate: Option<usize>,
    },
}

impl Error {
    const fn numerical(stage: Stage, coordinate: usize) -> Self {
        Self::Numerical {
            stage,
            coordinate: Some(coordinate),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DimensionMismatch { expected, found } => {
                write!(f, "expected {expected} features, found {found}")
            }
            Self::NonFiniteFeature { index } => write!(f, "feature {index} is not finite"),
            Self::NonFiniteTarget => f.write_str("target is not finite"),
            Self::Numerical {
                stage,
                coordinate: Some(i),
            } => write!(
                f,
                "numerical failure at the {stage} stage for coordinate {i}"
            ),
            Self::Numerical {
                stage,
                coordinate: None,
            } => write!(f, "numerical failure at the {stage} stage"),
        }
    }
}

impl error::Error for Error {}

const _: () = {
    const fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<Idbd>();
    assert_send_sync::<Error>();
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_messages_name_stage_and_coordinate() {
        assert_eq!(
            Error::numerical(Stage::StepSize, 3).to_string(),
            "numerical failure at the step size stage for coordinate 3"
        );
        assert_eq!(
            Error::Numerical {
                stage: Stage::Residual,
                coordinate: None
            }
            .to_string(),
            "numerical failure at the residual stage"
        );
        assert_eq!(
            Error::DimensionMismatch {
                expected: 3,
                found: 2
            }
            .to_string(),
            "expected 3 features, found 2"
        );
    }

    #[test]
    fn debug_omits_candidate_buffers() {
        let learner = Idbd::new(1, Config::new(0.1, 0.0).expect("valid")).expect("valid");
        let text = format!("{learner:?}");
        assert!(text.contains("weights"), "{text}");
        assert!(!text.contains("candidate"), "{text}");
    }
}
