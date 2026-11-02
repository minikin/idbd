//! A fixed-step LMS baseline with the same checked, atomic contract as the IDBD core.

use std::fmt;

/// Least mean squares with one fixed step size shared by every input.
///
/// `w_i = w_i + alpha * error * x_i`. Sutton (1992) compares IDBD against this rule.
#[derive(Debug, Clone)]
pub struct Lms {
    weights: Vec<f64>,
    step_size: f64,
}

/// Prediction and error measured before learning from one example.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Outcome {
    /// Prediction made before learning.
    pub prediction: f64,
    /// `target - prediction`.
    pub error: f64,
}

/// A failed LMS operation. The weights are unchanged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LmsError {
    /// The step size is not finite and strictly positive.
    InvalidStepSize,
    /// The feature vector has the wrong length.
    DimensionMismatch,
    /// A feature or the target is not finite.
    NonFiniteInput,
    /// The prediction or the residual is not finite.
    NonFiniteError,
    /// A new weight is not finite.
    NonFiniteWeight {
        /// The failed coordinate.
        coordinate: usize,
    },
}

impl fmt::Display for LmsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidStepSize => f.write_str("step size is not finite and positive"),
            Self::DimensionMismatch => f.write_str("feature vector has the wrong length"),
            Self::NonFiniteInput => f.write_str("a feature or the target is not finite"),
            Self::NonFiniteError => f.write_str("prediction or residual is not finite"),
            Self::NonFiniteWeight { coordinate } => {
                write!(f, "weight {coordinate} is not finite")
            }
        }
    }
}

impl std::error::Error for LmsError {}

impl Lms {
    /// Creates an LMS learner with zero weights.
    ///
    /// # Errors
    ///
    /// Returns [`LmsError::InvalidStepSize`] unless `step_size` is finite and positive.
    pub fn new(num_features: usize, step_size: f64) -> Result<Self, LmsError> {
        if !(step_size.is_finite() && step_size > 0.0) {
            return Err(LmsError::InvalidStepSize);
        }
        Ok(Self {
            weights: vec![0.0; num_features],
            step_size,
        })
    }

    /// Returns the step size.
    pub fn step_size(&self) -> f64 {
        self.step_size
    }

    /// Returns the weights.
    pub fn weights(&self) -> &[f64] {
        &self.weights
    }

    /// Validates an example without learning from it.
    ///
    /// A successful check guarantees that [`Lms::update`] with the same arguments
    /// succeeds, which lets callers pair LMS with another learner atomically.
    ///
    /// # Errors
    ///
    /// Returns the error that [`Lms::update`] would return.
    pub fn check(&self, features: &[f64], target: f64) -> Result<Outcome, LmsError> {
        if features.len() != self.weights.len() {
            return Err(LmsError::DimensionMismatch);
        }
        if !(target.is_finite() && features.iter().all(|x| x.is_finite())) {
            return Err(LmsError::NonFiniteInput);
        }
        let prediction = self
            .weights
            .iter()
            .zip(features)
            .fold(0.0, |sum, (w, x)| sum + w * x);
        let error = target - prediction;
        if !(prediction.is_finite() && error.is_finite()) {
            return Err(LmsError::NonFiniteError);
        }
        let candidates = self.weights.iter().zip(features);
        for (coordinate, (w, x)) in candidates.enumerate() {
            if !(w + self.step_size * error * x).is_finite() {
                return Err(LmsError::NonFiniteWeight { coordinate });
            }
        }
        Ok(Outcome { prediction, error })
    }

    /// Learns from one example.
    ///
    /// # Errors
    ///
    /// Returns an [`LmsError`] and leaves the weights unchanged if the example or any
    /// resulting weight is not finite.
    pub fn update(&mut self, features: &[f64], target: f64) -> Result<Outcome, LmsError> {
        let outcome = self.check(features, target)?;
        for (w, x) in self.weights.iter_mut().zip(features) {
            *w += self.step_size * outcome.error * x;
        }
        Ok(outcome)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn learns_with_a_fixed_step() {
        let mut lms = Lms::new(2, 0.1).expect("valid");
        let first = lms.update(&[1.0, 2.0], 1.0).expect("update");
        assert_eq!(
            first,
            Outcome {
                prediction: 0.0,
                error: 1.0
            }
        );
        assert!(
            (lms.weights()[0] - 0.1).abs() < 1e-15,
            "{:?}",
            lms.weights()
        );
        assert!(
            (lms.weights()[1] - 0.2).abs() < 1e-15,
            "{:?}",
            lms.weights()
        );
        let second = lms.update(&[1.0, 0.0], 1.0).expect("update");
        assert!((second.prediction - 0.1).abs() < 1e-15, "{second:?}");
    }

    #[test]
    fn rejects_without_changing_weights() {
        let mut lms = Lms::new(2, 0.5).expect("valid");
        lms.update(&[1.0, 1.0], 2.0).expect("update");
        let before = lms.weights().to_vec();
        let cases = [
            (vec![1.0], 1.0, LmsError::DimensionMismatch),
            (vec![f64::NAN, 1.0], 1.0, LmsError::NonFiniteInput),
            (vec![1.0, 1.0], f64::INFINITY, LmsError::NonFiniteInput),
            (vec![1e308, 1e308], 1.0, LmsError::NonFiniteError),
            (
                vec![1e300, 0.0],
                -1e300,
                LmsError::NonFiniteWeight { coordinate: 0 },
            ),
        ];
        for (x, target, expected) in cases {
            assert_eq!(lms.update(&x, target), Err(expected), "{x:?}");
            assert_eq!(lms.weights(), before.as_slice(), "{x:?}");
        }
        assert_eq!(Lms::new(1, 0.0).err(), Some(LmsError::InvalidStepSize));
        assert_eq!(Lms::new(1, f64::NAN).err(), Some(LmsError::InvalidStepSize));
    }

    #[test]
    fn error_messages_are_lowercase() {
        let message = LmsError::NonFiniteWeight { coordinate: 3 }.to_string();
        assert_eq!(message, "weight 3 is not finite");
    }
}
