//! Single learning runs on a seeded task, measured by pre-update squared error.

use idbd::{Config, Idbd};

use crate::lms::Lms;
use crate::world::{NUM_FEATURES, Preset, World};

/// The learner evaluated in a run.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LearnerSpec {
    /// IDBD with the given configuration.
    Idbd(Config),
    /// Fixed-step LMS with the given step size.
    Lms {
        /// The shared step size.
        step_size: f64,
    },
}

/// A run: warm up, then average the squared pre-update error over measured samples.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RunSpec {
    /// The task.
    pub preset: Preset,
    /// The task seed. Every learner given the same seed sees the same stream.
    pub seed: u64,
    /// Samples processed before measurement starts.
    pub warmup: u64,
    /// Samples whose squared pre-update error is averaged.
    pub measured: u64,
    /// The learner.
    pub learner: LearnerSpec,
}

/// The outcome of a run.
#[derive(Debug, Clone, PartialEq)]
pub struct RunResult {
    /// Mean squared pre-update error over the measured samples, if the run completed.
    pub mse: Option<f64>,
    /// The sample at which the learner failed, with the reason.
    pub failure: Option<(u64, String)>,
    /// Final step sizes, for learners that adapt them.
    pub final_step_sizes: Vec<f64>,
}

enum Learner {
    Idbd(Idbd),
    Lms(Lms),
}

impl Learner {
    fn update(&mut self, x: &[f64], target: f64) -> Result<f64, String> {
        match self {
            Self::Idbd(l) => l
                .update(x, target)
                .map(|r| r.error)
                .map_err(|e| e.to_string()),
            Self::Lms(l) => l
                .update(x, target)
                .map(|r| r.error)
                .map_err(|e| e.to_string()),
        }
    }
}

/// Runs one learner on one seeded stream.
///
/// # Errors
///
/// Returns a message if the learner configuration is invalid. Numerical failures
/// during the run are reported in [`RunResult::failure`].
pub fn run(spec: &RunSpec) -> Result<RunResult, String> {
    let mut learner = match spec.learner {
        LearnerSpec::Idbd(config) => {
            Learner::Idbd(Idbd::new(NUM_FEATURES, config).map_err(|e| e.to_string())?)
        }
        LearnerSpec::Lms { step_size } => {
            Learner::Lms(Lms::new(NUM_FEATURES, step_size).map_err(|e| e.to_string())?)
        }
    };
    let mut world = World::new(spec.preset, spec.seed);
    let mut x = [0.0; NUM_FEATURES];
    let mut events = Vec::new();
    let mut sum = 0.0;
    let mut failure = None;
    for t in 1..=spec.warmup + spec.measured {
        let target = world.next_sample(&mut x, &mut events);
        match learner.update(&x, target) {
            Ok(error) if t > spec.warmup => sum += error * error,
            Ok(_) => {}
            Err(message) => {
                failure = Some((t, message));
                break;
            }
        }
    }
    let final_step_sizes = match &learner {
        Learner::Idbd(l) => l.step_sizes().collect(),
        Learner::Lms(_) => Vec::new(),
    };
    let mse = failure
        .is_none()
        .then(|| sum / measured_as_f64(spec.measured));
    Ok(RunResult {
        mse: mse.filter(|m| m.is_finite()),
        failure,
        final_step_sizes,
    })
}

#[expect(
    clippy::cast_precision_loss,
    clippy::as_conversions,
    reason = "sample counts stay far below 2^53"
)]
fn measured_as_f64(count: u64) -> f64 {
    count as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(learner: LearnerSpec) -> RunSpec {
        RunSpec {
            preset: Preset::Sutton1992,
            seed: 1,
            warmup: 2_000,
            measured: 1_000,
            learner,
        }
    }

    #[test]
    fn idbd_and_lms_produce_finite_errors() {
        let idbd = run(&spec(LearnerSpec::Idbd(
            Config::new(0.05, 0.01).expect("valid"),
        )))
        .expect("valid");
        let lms = run(&spec(LearnerSpec::Lms { step_size: 0.04 })).expect("valid");
        assert!(idbd.mse.is_some_and(f64::is_finite), "{idbd:?}");
        assert!(lms.mse.is_some_and(f64::is_finite), "{lms:?}");
        assert_eq!(idbd.final_step_sizes.len(), NUM_FEATURES);
        assert!(lms.final_step_sizes.is_empty());
    }

    #[test]
    fn diverging_runs_report_their_failure() {
        let result = run(&spec(LearnerSpec::Lms { step_size: 5.0 })).expect("valid");
        assert!(result.mse.is_none(), "{result:?}");
        assert!(result.failure.is_some(), "{result:?}");
    }

    #[test]
    fn runs_are_reproducible() {
        let s = spec(LearnerSpec::Idbd(Config::new(0.05, 0.005).expect("valid")));
        assert_eq!(run(&s), run(&s));
    }
}
