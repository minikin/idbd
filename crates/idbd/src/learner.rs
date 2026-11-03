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
/// [`Clone::clone_from`] reuses the destination's buffers when it has the same number
/// of features, so a caller can keep a scratch copy and refresh it on every sample
/// without allocating.
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
pub struct Idbd {
    config: Config,
    committed: Vectors,
    /// Preallocated storage for the next state. Updates write only here and swap it
    /// with `committed` after every coordinate validates.
    candidate: Vectors,
}

/// Per-feature weights, log step sizes and traces.
struct Vectors {
    weights: Vec<f64>,
    betas: Vec<f64>,
    traces: Vec<f64>,
}

// Hand-written so that `clone_from` forwards to `Vec::clone_from`, which reuses the
// existing allocation. The derived impl would allocate fresh vectors every time.
impl Clone for Vectors {
    fn clone(&self) -> Self {
        Self {
            weights: self.weights.clone(),
            betas: self.betas.clone(),
            traces: self.traces.clone(),
        }
    }

    fn clone_from(&mut self, source: &Self) {
        self.weights.clone_from(&source.weights);
        self.betas.clone_from(&source.betas);
        self.traces.clone_from(&source.traces);
    }
}

impl Clone for Idbd {
    fn clone(&self) -> Self {
        Self {
            config: self.config,
            committed: self.committed.clone(),
            candidate: self.candidate.clone(),
        }
    }

    // The candidate buffer holds no state between updates, so only its length matters.
    fn clone_from(&mut self, source: &Self) {
        self.config = source.config;
        self.committed.clone_from(&source.committed);
        if self.candidate.weights.len() != source.candidate.weights.len() {
            self.candidate.clone_from(&source.candidate);
        }
    }
}

impl Vectors {
    fn new(num_features: usize, beta: f64) -> Self {
        Self {
            weights: vec![0.0; num_features],
            betas: vec![beta; num_features],
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
            committed: Vectors::new(num_features, config.initial_beta()),
            candidate: Vectors::new(num_features, 0.0),
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
        let candidate = Vectors::new(weights.len(), 0.0);
        Ok(Self {
            config,
            committed: Vectors {
                weights,
                betas,
                traces,
            },
            candidate,
        })
    }

    /// Returns a copy of the complete learning state.
    ///
    /// This allocates three vectors. Pass the result to [`Idbd::from_state`] to resume.
    pub fn snapshot(&self) -> State {
        State::new(
            self.config,
            self.committed.weights.clone(),
            self.committed.betas.clone(),
            self.committed.traces.clone(),
        )
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
    /// On error the committed state is unchanged. Checks run in this order and the
    /// first failure is returned:
    ///
    /// 1. [`Error::DimensionMismatch`] if `features` has the wrong length.
    /// 2. [`Error::NonFiniteFeature`] for the first non-finite feature.
    /// 3. [`Error::NonFiniteTarget`] if `target` is not finite.
    /// 4. [`Error::Numerical`] at [`Stage::Prediction`], then [`Stage::Residual`].
    /// 5. [`Error::Numerical`] for the lowest failing coordinate, at the first failing
    ///    stage among [`Stage::MetaStep`], [`Stage::StepSize`], [`Stage::Weight`] and
    ///    [`Stage::Trace`].
    ///
    /// Products are evaluated left to right as written in the equations, for example
    /// `theta * error * x_i * h_i`. An intermediate product that overflows is a
    /// numerical failure even if reordering the factors would avoid it.
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
        let bounded_features = propose(
            &self.config,
            &self.committed,
            &mut self.candidate,
            features,
            error,
        )?;
        mem::swap(&mut self.committed, &mut self.candidate);
        Ok(Update {
            prediction,
            error,
            bounded_features,
        })
    }

    fn check_features(&self, features: &[f64]) -> Result<(), Error> {
        check_features(self.committed.weights.len(), features)
    }

    fn dot(&self, features: &[f64]) -> Result<f64, Error> {
        dot(&self.committed.weights, features)
    }

    /// Returns the number of input features.
    pub fn num_features(&self) -> usize {
        self.committed.weights.len()
    }

    /// Returns the configuration.
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Returns the committed weights.
    pub fn weights(&self) -> &[f64] {
        &self.committed.weights
    }

    /// Returns the committed log step sizes, beta.
    pub fn log_step_sizes(&self) -> &[f64] {
        &self.committed.betas
    }

    /// Returns the committed adaptation traces, h.
    pub fn traces(&self) -> &[f64] {
        &self.committed.traces
    }

    /// Returns an iterator over the step sizes `alpha_i = exp(beta_i)`.
    ///
    /// The iterator computes each value on demand and does not allocate.
    pub fn step_sizes(&self) -> StepSizes<'_> {
        StepSizes {
            betas: self.committed.betas.iter(),
        }
    }
}

/// One coordinate's weight, log step size and trace.
/// Checks the feature count, then that every feature is finite.
pub(crate) fn check_features(num_features: usize, features: &[f64]) -> Result<(), Error> {
    if features.len() != num_features {
        return Err(Error::DimensionMismatch {
            expected: num_features,
            found: features.len(),
        });
    }
    match features.iter().position(|x| !x.is_finite()) {
        Some(index) => Err(Error::NonFiniteFeature { index }),
        None => Ok(()),
    }
}

/// Returns `w · x`, summed left to right.
pub(crate) fn dot(weights: &[f64], features: &[f64]) -> Result<f64, Error> {
    let prediction = weights
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

#[derive(Clone, Copy)]
struct Coordinate {
    weight: f64,
    beta: f64,
    trace: f64,
}

/// Writes the next state into `candidate` and returns how many coordinates the bounds
/// changed. Committed state is only borrowed, so a failure cannot modify it.
fn propose(
    config: &Config,
    committed: &Vectors,
    candidate: &mut Vectors,
    features: &[f64],
    error: f64,
) -> Result<usize, Error> {
    let old = committed
        .weights
        .iter()
        .zip(&committed.betas)
        .zip(&committed.traces);
    let new = candidate
        .weights
        .iter_mut()
        .zip(&mut candidate.betas)
        .zip(&mut candidate.traces);
    let mut bounded_features = 0;
    for (i, ((&x, ((&weight, &beta), &trace)), ((new_weight, new_beta), new_trace))) in
        features.iter().zip(old).zip(new).enumerate()
    {
        let old = Coordinate {
            weight,
            beta,
            trace,
        };
        let (next, bounded) =
            next_coordinate(config, error, x, old).map_err(|stage| Error::numerical(stage, i))?;
        bounded_features += usize::from(bounded);
        *new_weight = next.weight;
        *new_beta = next.beta;
        *new_trace = next.trace;
    }
    Ok(bounded_features)
}

/// Applies equations 3 to 6 to one coordinate. Returns the next values and whether the
/// bounds changed beta, or the stage that failed.
fn next_coordinate(
    config: &Config,
    error: f64,
    x: f64,
    old: Coordinate,
) -> Result<(Coordinate, bool), Stage> {
    // A zero input changes nothing mathematically. Skipping it also keeps an overflowing
    // `alpha * error` from turning `* 0.0` into NaN.
    if x == 0.0 {
        return Ok((old, false));
    }
    let raw_increment = config.meta_step_size() * error * x * old.trace;
    let proposal = old.beta + raw_increment;
    // Committed betas never exceed ln(f64::MAX), so the proposal is finite exactly when
    // the raw increment is.
    if !proposal.is_finite() {
        return Err(Stage::MetaStep);
    }
    let beta = config.bounds().apply(old.beta, raw_increment);
    #[expect(
        clippy::float_cmp,
        reason = "the contract counts any floating-point difference from the proposal"
    )]
    let bounded = beta != proposal;
    let alpha = beta.exp();
    if !is_valid_step_size(alpha) {
        return Err(Stage::StepSize);
    }
    let step = alpha * error * x;
    let weight = old.weight + step;
    if !weight.is_finite() {
        return Err(Stage::Weight);
    }
    let decay = (1.0 - alpha * x * x).max(0.0);
    let trace = old.trace * decay + step;
    if !trace.is_finite() {
        return Err(Stage::Trace);
    }
    Ok((
        Coordinate {
            weight,
            beta,
            trace,
        },
        bounded,
    ))
}

impl fmt::Debug for Idbd {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Idbd")
            .field("config", &self.config)
            .field("weights", &self.committed.weights)
            .field("betas", &self.committed.betas)
            .field("traces", &self.committed.traces)
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
    /// Autostep only: the running normalizer `v_i` of the step-size update.
    Normalizer,
    /// Autostep only: the effective step size `sum(alpha_i * x_i^2)`.
    EffectiveStepSize,
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
            Self::Normalizer => "normalizer",
            Self::EffectiveStepSize => "effective step size",
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
    pub(crate) const fn numerical(stage: Stage, coordinate: usize) -> Self {
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
