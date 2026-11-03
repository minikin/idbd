//! Autostep: IDBD with a normalized meta step and an overshoot guard.
//!
//! Mahmood, Sutton, Degris and Pilarski (2012) derive Autostep from IDBD with two
//! changes. Each step-size update `delta * x_i * h_i` is divided by `v_i`, a running
//! maximum of its absolute value, so the meta step size `mu` has no units and one
//! value (about `0.01`) works across problems. Then, if the effective step size
//! `sum(alpha_i * x_i^2)` exceeds one, every step size is divided by it, so an update
//! never overshoots the target of the current example.
//!
//! [`Autostep`] implements Table 1 of the paper exactly, with the same validation,
//! atomic updates and allocation-free learning path as [`crate::Idbd`]. For each example
//! `(x, y)`:
//!
//! ```text
//! delta = y - sum(w_i * x_i)
//! for each i:
//!     v_i     = max(|delta x_i h_i|, v_i + (1 / tau) alpha_i x_i^2 (|delta x_i h_i| - v_i))
//!     if v_i != 0:
//!         alpha_i = alpha_i * exp(mu * delta x_i h_i / v_i)
//! M = max(sum(alpha_i x_i^2), 1)
//! for each i:
//!     alpha_i = alpha_i / M
//!     w_i     = w_i + alpha_i delta x_i
//!     h_i     = h_i (1 - alpha_i x_i^2) + alpha_i delta x_i
//! ```
//!
//! Division by `M` applies to every step size, including those of inputs that are zero
//! in the current example.
//!
//! # Example
//!
//! ```
//! use idbd::autostep::{Autostep, Config};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // Raw, unscaled targets: Autostep's normalization keeps the step sizes in range.
//! let mut learner = Autostep::new(2, Config::new(0.1)?)?;
//! for _ in 0..200 {
//!     learner.update(&[1.0, 30.0], 4_000.0)?;
//! }
//! let prediction = learner.predict(&[1.0, 30.0])?;
//! assert!((prediction - 4_000.0).abs() < 1e-6);
//! # Ok(())
//! # }
//! ```
//!
//! # Reference
//!
//! A. Rupam Mahmood, Richard S. Sutton, Thomas Degris and Patrick M. Pilarski.
//! *Tuning-free step-size adaptation.* IEEE International Conference on Acoustics,
//! Speech and Signal Processing (ICASSP), pp. 2121-2124, 2012.
//! <https://doi.org/10.1109/ICASSP.2012.6288330>

use std::error;
use std::fmt;
use std::mem;

use crate::config::is_valid_step_size;
use crate::learner::{check_features, dot};
use crate::{ConfigError, Error, Stage};

/// Validated Autostep parameters.
///
/// # Examples
///
/// ```
/// use idbd::autostep::Config;
///
/// # fn main() -> Result<(), idbd::ConfigError> {
/// let config = Config::new(0.1)?.with_time_scale(1_000.0)?;
/// assert_eq!(config.meta_step_size(), Config::DEFAULT_META_STEP_SIZE);
/// assert_eq!(config.time_scale(), 1_000.0);
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Config {
    initial_step_size: f64,
    meta_step_size: f64,
    time_scale: f64,
}

impl Config {
    /// The meta step size `mu` that the paper found near-best on every problem it
    /// tested: `0.01`.
    pub const DEFAULT_META_STEP_SIZE: f64 = 0.01;

    /// The time scale `tau` used in all of the paper's experiments: `10_000` samples.
    /// The paper suggests the number of samples expected to be needed for learning.
    pub const DEFAULT_TIME_SCALE: f64 = 10_000.0;

    /// Creates a configuration with the paper's default meta step size and time scale.
    ///
    /// The paper reports that performance is nearly insensitive to the initial step
    /// size, and suggests `0.1` divided by the expected norm of the input vectors.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::InvalidInitialStepSize`] unless `initial_step_size` is
    /// finite and strictly positive.
    pub fn new(initial_step_size: f64) -> Result<Self, ConfigError> {
        if !is_valid_step_size(initial_step_size) {
            return Err(ConfigError::InvalidInitialStepSize {
                value: initial_step_size,
            });
        }
        Ok(Self {
            initial_step_size,
            meta_step_size: Self::DEFAULT_META_STEP_SIZE,
            time_scale: Self::DEFAULT_TIME_SCALE,
        })
    }

    /// Replaces the meta step size `mu`. Zero disables step-size adaptation, apart
    /// from the overshoot guard.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::InvalidMetaStepSize`] unless `meta_step_size` is finite
    /// and nonnegative.
    pub fn with_meta_step_size(self, meta_step_size: f64) -> Result<Self, ConfigError> {
        if !(meta_step_size.is_finite() && meta_step_size >= 0.0) {
            return Err(ConfigError::InvalidMetaStepSize {
                value: meta_step_size,
            });
        }
        Ok(Self {
            meta_step_size,
            ..self
        })
    }

    /// Replaces the time scale `tau` of the normalizers, in samples.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::InvalidTimeScale`] unless `time_scale` is finite and at
    /// least `1`.
    pub fn with_time_scale(self, time_scale: f64) -> Result<Self, ConfigError> {
        if !(time_scale.is_finite() && time_scale >= 1.0) {
            return Err(ConfigError::InvalidTimeScale { value: time_scale });
        }
        Ok(Self { time_scale, ..self })
    }

    /// Returns the initial step size of every input.
    pub const fn initial_step_size(&self) -> f64 {
        self.initial_step_size
    }

    /// Returns the meta step size, `mu`.
    pub const fn meta_step_size(&self) -> f64 {
        self.meta_step_size
    }

    /// Returns the time scale of the normalizers, `tau`.
    pub const fn time_scale(&self) -> f64 {
        self.time_scale
    }
}

/// A dense linear learner that adapts one step size per input with Autostep.
///
/// See the [module documentation](self) for the update equations.
///
/// [`Clone::clone_from`] reuses the destination's buffers when it has the same number
/// of features, so a caller can keep a scratch copy and refresh it on every sample
/// without allocating.
pub struct Autostep {
    config: Config,
    committed: Vectors,
    /// Preallocated storage for the next state. Updates write only here and swap it
    /// with `committed` after every coordinate validates.
    candidate: Vectors,
}

struct Vectors {
    weights: Vec<f64>,
    step_sizes: Vec<f64>,
    traces: Vec<f64>,
    normalizers: Vec<f64>,
}

// Hand-written so that `clone_from` forwards to `Vec::clone_from`, which reuses the
// existing allocation. The derived impl would allocate fresh vectors every time.
impl Clone for Vectors {
    fn clone(&self) -> Self {
        Self {
            weights: self.weights.clone(),
            step_sizes: self.step_sizes.clone(),
            traces: self.traces.clone(),
            normalizers: self.normalizers.clone(),
        }
    }

    fn clone_from(&mut self, source: &Self) {
        self.weights.clone_from(&source.weights);
        self.step_sizes.clone_from(&source.step_sizes);
        self.traces.clone_from(&source.traces);
        self.normalizers.clone_from(&source.normalizers);
    }
}

impl Clone for Autostep {
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
    fn new(num_features: usize, step_size: f64) -> Self {
        Self {
            weights: vec![0.0; num_features],
            step_sizes: vec![step_size; num_features],
            traces: vec![0.0; num_features],
            normalizers: vec![0.0; num_features],
        }
    }
}

/// Report of a successful [`Autostep::update`].
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub struct Update {
    /// Prediction made before learning from the example.
    pub prediction: f64,
    /// `target - prediction`, measured before learning.
    pub error: f64,
    /// `sum(alpha_i * x_i^2)` after the step-size update and before the overshoot
    /// guard. When it exceeds one, every step size was divided by it.
    pub effective_step_size: f64,
}

impl Autostep {
    /// Creates a learner for `num_features` dense inputs, with zero weights, traces and
    /// normalizers.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::ZeroFeatures`] if `num_features` is zero.
    pub fn new(num_features: usize, config: Config) -> Result<Self, ConfigError> {
        if num_features == 0 {
            return Err(ConfigError::ZeroFeatures);
        }
        Ok(Self {
            committed: Vectors::new(num_features, config.initial_step_size()),
            candidate: Vectors::new(num_features, 0.0),
            config,
        })
    }

    /// Restores a learner from a complete state, validating every value.
    ///
    /// # Errors
    ///
    /// Returns a [`StateError`] if the vectors are empty or differ in length, if any
    /// value is not finite, if a step size is not strictly positive, or if a
    /// normalizer is negative.
    pub fn from_state(state: State) -> Result<Self, StateError> {
        state.validate()?;
        let State {
            config,
            weights,
            step_sizes,
            traces,
            normalizers,
        } = state;
        let candidate = Vectors::new(weights.len(), 0.0);
        Ok(Self {
            config,
            committed: Vectors {
                weights,
                step_sizes,
                traces,
                normalizers,
            },
            candidate,
        })
    }

    /// Returns a copy of the complete learning state. This allocates four vectors.
    pub fn snapshot(&self) -> State {
        State::new(
            self.config,
            self.committed.weights.clone(),
            self.committed.step_sizes.clone(),
            self.committed.traces.clone(),
            self.committed.normalizers.clone(),
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
        check_features(self.num_features(), features)?;
        dot(&self.committed.weights, features)
    }

    /// Learns from one example and reports the prediction made before learning.
    ///
    /// # Errors
    ///
    /// On error the committed state is unchanged. Checks run in this order and the
    /// first failure is returned:
    ///
    /// 1. [`Error::DimensionMismatch`], [`Error::NonFiniteFeature`] and
    ///    [`Error::NonFiniteTarget`], as for [`crate::Idbd::update`].
    /// 2. [`Error::Numerical`] at [`Stage::Prediction`], then [`Stage::Residual`].
    /// 3. For the lowest failing coordinate, the first failing stage among
    ///    [`Stage::MetaStep`] (`delta * x_i * h_i`), [`Stage::Normalizer`] and
    ///    [`Stage::StepSize`].
    /// 4. [`Stage::EffectiveStepSize`] if `sum(alpha_i * x_i^2)` is not finite.
    /// 5. For the lowest failing coordinate, the first failing stage among
    ///    [`Stage::StepSize`] (after the overshoot guard), [`Stage::Weight`] and
    ///    [`Stage::Trace`].
    ///
    /// Products are evaluated left to right as written in the equations.
    pub fn update(&mut self, features: &[f64], target: f64) -> Result<Update, Error> {
        check_features(self.num_features(), features)?;
        if !target.is_finite() {
            return Err(Error::NonFiniteTarget);
        }
        let prediction = dot(&self.committed.weights, features)?;
        let error = target - prediction;
        if !error.is_finite() {
            return Err(Error::Numerical {
                stage: Stage::Residual,
                coordinate: None,
            });
        }
        let effective_step_size = adapt_step_sizes(
            &self.config,
            &self.committed,
            &mut self.candidate,
            features,
            error,
        )?;
        learn(
            &self.committed,
            &mut self.candidate,
            features,
            error,
            effective_step_size.max(1.0),
        )?;
        mem::swap(&mut self.committed, &mut self.candidate);
        Ok(Update {
            prediction,
            error,
            effective_step_size,
        })
    }

    /// Returns the number of input features.
    pub fn num_features(&self) -> usize {
        self.committed.weights.len()
    }

    /// Returns the configuration.
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Returns the weights.
    pub fn weights(&self) -> &[f64] {
        &self.committed.weights
    }

    /// Returns the step sizes, `alpha_i`.
    pub fn step_sizes(&self) -> &[f64] {
        &self.committed.step_sizes
    }

    /// Returns the traces, `h_i`.
    pub fn traces(&self) -> &[f64] {
        &self.committed.traces
    }

    /// Returns the normalizers, `v_i`.
    pub fn normalizers(&self) -> &[f64] {
        &self.committed.normalizers
    }
}

/// First loop of Table 1: writes the new normalizers and unguarded step sizes into
/// `candidate` and returns `sum(alpha_i * x_i^2)`.
fn adapt_step_sizes(
    config: &Config,
    committed: &Vectors,
    candidate: &mut Vectors,
    features: &[f64],
    error: f64,
) -> Result<f64, Error> {
    let mu = config.meta_step_size();
    let rate = 1.0 / config.time_scale();
    let mut effective_step_size = 0.0;
    for (i, &x) in features.iter().enumerate() {
        let (alpha, v) = (committed.step_sizes[i], committed.normalizers[i]);
        // A zero input leaves v and alpha unchanged in exact arithmetic. Skipping it
        // keeps an overflowing `delta * x_i` from turning `* 0.0` into NaN.
        let (alpha, v) = if x == 0.0 {
            (alpha, v)
        } else {
            let g = error * x * committed.traces[i];
            if !g.is_finite() {
                return Err(Error::numerical(Stage::MetaStep, i));
            }
            let v = g.abs().max(v + rate * alpha * x * x * (g.abs() - v));
            if !v.is_finite() {
                return Err(Error::numerical(Stage::Normalizer, i));
            }
            let alpha = if v == 0.0 {
                alpha
            } else {
                alpha * (mu * g / v).exp()
            };
            if !is_valid_step_size(alpha) {
                return Err(Error::numerical(Stage::StepSize, i));
            }
            (alpha, v)
        };
        candidate.step_sizes[i] = alpha;
        candidate.normalizers[i] = v;
        effective_step_size += alpha * x * x;
    }
    if effective_step_size.is_finite() {
        Ok(effective_step_size)
    } else {
        Err(Error::Numerical {
            stage: Stage::EffectiveStepSize,
            coordinate: None,
        })
    }
}

/// Second loop of Table 1: applies the overshoot guard `m` to the step sizes in
/// `candidate`, then writes the new weights and traces.
fn learn(
    committed: &Vectors,
    candidate: &mut Vectors,
    features: &[f64],
    error: f64,
    m: f64,
) -> Result<(), Error> {
    for (i, &x) in features.iter().enumerate() {
        let alpha = candidate.step_sizes[i] / m;
        if !is_valid_step_size(alpha) {
            return Err(Error::numerical(Stage::StepSize, i));
        }
        candidate.step_sizes[i] = alpha;
        let (weight, trace) = (committed.weights[i], committed.traces[i]);
        // As above: a zero input changes neither, and skipping it avoids `inf * 0.0`.
        if x == 0.0 {
            candidate.weights[i] = weight;
            candidate.traces[i] = trace;
            continue;
        }
        let step = alpha * error * x;
        let weight = weight + step;
        if !weight.is_finite() {
            return Err(Error::numerical(Stage::Weight, i));
        }
        let trace = trace * (1.0 - alpha * x * x) + step;
        if !trace.is_finite() {
            return Err(Error::numerical(Stage::Trace, i));
        }
        candidate.weights[i] = weight;
        candidate.traces[i] = trace;
    }
    Ok(())
}

impl fmt::Debug for Autostep {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Autostep")
            .field("config", &self.config)
            .field("weights", &self.committed.weights)
            .field("step_sizes", &self.committed.step_sizes)
            .field("traces", &self.committed.traces)
            .field("normalizers", &self.committed.normalizers)
            .finish_non_exhaustive()
    }
}

impl TryFrom<State> for Autostep {
    type Error = StateError;

    fn try_from(state: State) -> Result<Self, Self::Error> {
        Self::from_state(state)
    }
}

/// The complete state of an [`Autostep`] learner.
///
/// [`Autostep::snapshot`] produces one and [`Autostep::from_state`] validates and
/// restores it. All four vectors are needed to continue the same learning process.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct State {
    /// The learner configuration.
    pub config: Config,
    /// One weight per feature.
    pub weights: Vec<f64>,
    /// One step size per feature, `alpha_i`.
    pub step_sizes: Vec<f64>,
    /// One trace per feature, `h_i`.
    pub traces: Vec<f64>,
    /// One normalizer per feature, `v_i`.
    pub normalizers: Vec<f64>,
}

impl State {
    /// Assembles a state from its parts without validating them.
    ///
    /// [`Autostep::from_state`] validates the state when it restores a learner.
    pub fn new(
        config: Config,
        weights: Vec<f64>,
        step_sizes: Vec<f64>,
        traces: Vec<f64>,
        normalizers: Vec<f64>,
    ) -> Self {
        Self {
            config,
            weights,
            step_sizes,
            traces,
            normalizers,
        }
    }

    fn validate(&self) -> Result<(), StateError> {
        let lengths = [
            self.weights.len(),
            self.step_sizes.len(),
            self.traces.len(),
            self.normalizers.len(),
        ];
        if lengths.iter().any(|&len| len != lengths[0]) {
            return Err(StateError::LengthMismatch {
                weights: lengths[0],
                step_sizes: lengths[1],
                traces: lengths[2],
                normalizers: lengths[3],
            });
        }
        if lengths[0] == 0 {
            return Err(StateError::Empty);
        }
        let rows = self
            .weights
            .iter()
            .zip(&self.step_sizes)
            .zip(&self.traces)
            .zip(&self.normalizers);
        for (index, (((&weight, &step_size), &trace), &normalizer)) in rows.enumerate() {
            if !weight.is_finite() {
                return Err(StateError::NonFiniteWeight { index });
            }
            if !is_valid_step_size(step_size) {
                return Err(StateError::InvalidStepSize { index, step_size });
            }
            if !trace.is_finite() {
                return Err(StateError::NonFiniteTrace { index });
            }
            if !(normalizer.is_finite() && normalizer >= 0.0) {
                return Err(StateError::InvalidNormalizer { index, normalizer });
            }
        }
        Ok(())
    }
}

/// An Autostep [`State`] that cannot be restored.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum StateError {
    /// The state vectors are empty.
    Empty,
    /// The state vectors differ in length.
    LengthMismatch {
        /// Number of weights.
        weights: usize,
        /// Number of step sizes.
        step_sizes: usize,
        /// Number of traces.
        traces: usize,
        /// Number of normalizers.
        normalizers: usize,
    },
    /// A weight is NaN or infinite.
    NonFiniteWeight {
        /// The feature index.
        index: usize,
    },
    /// A step size is not finite and strictly positive.
    InvalidStepSize {
        /// The feature index.
        index: usize,
        /// The rejected step size.
        step_size: f64,
    },
    /// A trace is NaN or infinite.
    NonFiniteTrace {
        /// The feature index.
        index: usize,
    },
    /// A normalizer is negative or not finite.
    InvalidNormalizer {
        /// The feature index.
        index: usize,
        /// The rejected normalizer.
        normalizer: f64,
    },
}

impl fmt::Display for StateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("state has no features"),
            Self::LengthMismatch {
                weights,
                step_sizes,
                traces,
                normalizers,
            } => write!(
                f,
                "state vectors differ in length: {weights} weights, {step_sizes} step sizes, {traces} traces, {normalizers} normalizers"
            ),
            Self::NonFiniteWeight { index } => write!(f, "weight {index} is not finite"),
            Self::InvalidStepSize { index, step_size } => write!(
                f,
                "step size {index} ({step_size}) is not finite and strictly positive"
            ),
            Self::NonFiniteTrace { index } => write!(f, "trace {index} is not finite"),
            Self::InvalidNormalizer { index, normalizer } => write!(
                f,
                "normalizer {index} ({normalizer}) is not finite and nonnegative"
            ),
        }
    }
}

impl error::Error for StateError {}
