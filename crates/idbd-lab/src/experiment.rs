//! Single learning runs on a seeded task, measured by pre-update squared error.

use idbd::autostep::{self, Autostep};
use idbd::{Config, Idbd};

use crate::lms::Lms;
use crate::task::{Stream, Task};

/// The learner evaluated in a run.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LearnerSpec {
    /// IDBD with the given configuration.
    Idbd(Config),
    /// Autostep with the given configuration.
    Autostep(autostep::Config),
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
    pub task: Task,
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
    Autostep(Autostep),
    Lms(Lms),
}

impl Learner {
    fn new(spec: LearnerSpec, num_features: usize) -> Result<Self, String> {
        match spec {
            LearnerSpec::Idbd(config) => Idbd::new(num_features, config)
                .map(Self::Idbd)
                .map_err(|e| e.to_string()),
            LearnerSpec::Autostep(config) => Autostep::new(num_features, config)
                .map(Self::Autostep)
                .map_err(|e| e.to_string()),
            LearnerSpec::Lms { step_size } => Lms::new(num_features, step_size)
                .map(Self::Lms)
                .map_err(|e| e.to_string()),
        }
    }

    fn final_step_sizes(&self) -> Vec<f64> {
        match self {
            Self::Idbd(l) => l.step_sizes().collect(),
            Self::Autostep(l) => l.step_sizes().to_vec(),
            Self::Lms(_) => Vec::new(),
        }
    }

    fn update(&mut self, x: &[f64], target: f64) -> Result<f64, String> {
        match self {
            Self::Idbd(l) => l
                .update(x, target)
                .map(|r| r.error)
                .map_err(|e| e.to_string()),
            Self::Autostep(l) => l
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
    let mut learner = Learner::new(spec.learner, spec.task.num_features())?;
    let mut stream = Stream::new(spec.task, spec.seed);
    let mut x = vec![0.0; spec.task.num_features()];
    let mut sum = 0.0;
    let mut failure = None;
    for t in 1..=spec.warmup + spec.measured {
        let target = stream.next_sample(&mut x);
        match learner.update(&x, target) {
            Ok(error) if t > spec.warmup => sum += error * error,
            Ok(_) => {}
            Err(message) => {
                failure = Some((t, message));
                break;
            }
        }
    }
    let final_step_sizes = learner.final_step_sizes();
    // A squared error above 1.3e154 overflows the sum while the weights stay finite, so
    // the run completes every sample with no measurable mean. That is a failure of the
    // measurement, not a missing value: recording it as `None` alone would hide the
    // divergence from `Point::failures` and let `sweep::best` select the value.
    let mut mse = failure
        .is_none()
        .then(|| sum / measured_as_f64(spec.measured));
    if failure.is_none() && !mse.is_some_and(f64::is_finite) {
        failure = Some((
            spec.warmup + spec.measured,
            format!(
                "no finite mean over {} measured samples: squared errors sum to {sum}",
                spec.measured
            ),
        ));
        mse = None;
    }
    Ok(RunResult {
        mse,
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
    use crate::world::{NUM_FEATURES, Preset, World};

    fn spec(learner: LearnerSpec) -> RunSpec {
        RunSpec {
            task: Task::Preset(Preset::Sutton1992),
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
    fn autostep_reports_its_error_and_final_step_sizes() {
        let config = idbd::autostep::Config::new(0.005).expect("valid");
        let result = run(&spec(LearnerSpec::Autostep(config))).expect("valid");
        assert!(
            result.mse.is_some_and(|m| m.is_finite() && m > 0.0),
            "{result:?}"
        );
        assert_eq!(result.final_step_sizes.len(), NUM_FEATURES);
        assert!(
            result
                .final_step_sizes
                .iter()
                .any(|a| a.to_bits() != 0.005_f64.to_bits()),
            "step sizes adapt"
        );
    }

    #[test]
    fn diverging_runs_report_their_failure() {
        let result = run(&spec(LearnerSpec::Lms { step_size: 5.0 })).expect("valid");
        assert!(result.mse.is_none(), "{result:?}");
        assert!(result.failure.is_some(), "{result:?}");
    }

    #[test]
    fn mse_averages_the_measured_window_only() {
        let spec = RunSpec {
            task: Task::Preset(Preset::Sutton1992),
            seed: 5,
            warmup: 30,
            measured: 7,
            learner: LearnerSpec::Lms { step_size: 0.04 },
        };
        let measured = run(&spec).expect("valid").mse.expect("completed");

        let mut world = World::new(Preset::Sutton1992, spec.seed);
        let mut lms = Lms::new(NUM_FEATURES, 0.04).expect("valid");
        let mut x = [0.0; NUM_FEATURES];
        let mut events = Vec::new();
        let mut sum = 0.0;
        let mut counted = 0_u64;
        let mut warmup_sum = 0.0;
        for t in 1..=37_u64 {
            let target = world.next_sample(&mut x, &mut events);
            let error = lms.update(&x, target).expect("finite").error;
            if t > 30 {
                sum += error * error;
                counted += 1;
            } else {
                warmup_sum += error * error;
            }
        }
        assert_eq!(counted, 7, "samples 31..=37 are measured");
        assert!(warmup_sum > 0.0, "the warmup errors are nonzero");
        assert_eq!(
            measured.to_bits(),
            (sum / 7.0).to_bits(),
            "{measured} vs {}",
            sum / 7.0
        );
    }

    #[test]
    fn an_overflowing_error_sum_is_a_failure() {
        // With this step size the squared error overflows at sample 147 while the weights
        // stay finite until sample 289, so every measured sample completes and the mean
        // is still not a number.
        let result = run(&RunSpec {
            task: Task::Preset(Preset::Sutton1992),
            seed: 1,
            warmup: 100,
            measured: 100,
            learner: LearnerSpec::Lms { step_size: 5.0 },
        })
        .expect("valid");
        assert!(result.mse.is_none(), "{result:?}");
        let (sample, reason) = result.failure.expect("an unmeasurable run is a failure");
        assert_eq!(
            sample, 200,
            "the run reached the end of the measured window"
        );
        assert!(reason.contains("no finite mean"), "{reason}");
    }

    #[test]
    fn a_run_without_measured_samples_is_a_failure() {
        let result = run(&RunSpec {
            task: Task::Preset(Preset::Stationary),
            seed: 1,
            warmup: 10,
            measured: 0,
            learner: LearnerSpec::Lms { step_size: 0.04 },
        })
        .expect("valid");
        assert!(result.mse.is_none(), "{result:?}");
        assert!(result.failure.is_some(), "{result:?}");
    }

    #[test]
    fn runs_are_reproducible() {
        let s = spec(LearnerSpec::Idbd(Config::new(0.05, 0.005).expect("valid")));
        assert_eq!(run(&s), run(&s));
    }
}
