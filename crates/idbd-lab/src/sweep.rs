//! Parameter sweeps: one learner family over a recorded grid of values and seeds.
//!
//! A sweep never averages away a failure. Each grid value keeps its per-seed results,
//! and [`best`] refuses to select a value whose runs did not all complete.

use idbd::{Bounds, Config};

use crate::experiment::{LearnerSpec, RunSpec, run};
use crate::lms::Lms;
use crate::stats::Summary;
use crate::world::Preset;

/// Which learner a sweep varies, and what the swept value means.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Family {
    /// Fixed-step LMS. The swept value is the step size alpha.
    Lms,
    /// IDBD. The swept value is the meta step size theta.
    Idbd {
        /// Starting step size of every input, `alpha_0`.
        initial_step_size: f64,
        /// Optional beta bounds. Paper reproductions use [`Bounds::none`].
        bounds: Bounds,
    },
}

impl Family {
    /// Returns a stable machine-readable learner identifier.
    pub const fn learner_id(self) -> &'static str {
        match self {
            Self::Lms => "lms",
            Self::Idbd { .. } => "idbd",
        }
    }

    /// Returns the name of the swept parameter, for result columns.
    pub const fn parameter(self) -> &'static str {
        match self {
            Self::Lms => "alpha",
            Self::Idbd { .. } => "theta",
        }
    }

    /// Builds the learner for one grid value.
    ///
    /// # Errors
    ///
    /// Returns a message if the value does not produce a valid configuration.
    pub fn learner(self, value: f64) -> Result<LearnerSpec, String> {
        match self {
            Self::Lms => Lms::new(1, value)
                .map(|_| LearnerSpec::Lms { step_size: value })
                .map_err(|e| e.to_string()),
            Self::Idbd {
                initial_step_size,
                bounds,
            } => {
                let config = Config::new(initial_step_size, value)
                    .and_then(|c| c.with_bounds(bounds))
                    .map_err(|e| e.to_string())?;
                Ok(LearnerSpec::Idbd(config))
            }
        }
    }
}

/// The measurement protocol shared by every run of a sweep.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Protocol {
    /// The task.
    pub preset: Preset,
    /// Samples processed before measurement starts.
    pub warmup: u64,
    /// Samples whose squared pre-update error is averaged.
    pub measured: u64,
}

/// Why and where a run stopped early.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failure {
    /// The sample index, counting from one, at which the learner refused to update.
    pub sample: u64,
    /// The learner's own error message.
    pub reason: String,
}

/// One seed's result for one grid value.
#[derive(Debug, Clone, PartialEq)]
pub struct SeedRun {
    /// The task seed.
    pub seed: u64,
    /// Mean squared pre-update error over the measured samples, if the run completed.
    pub mse: Option<f64>,
    /// The failure that ended the run, if any.
    pub failure: Option<Failure>,
}

/// Every seed's result for one grid value.
#[derive(Debug, Clone, PartialEq)]
pub struct Point {
    /// The swept parameter value.
    pub value: f64,
    /// One entry per seed, in the order the seeds were given.
    pub runs: Vec<SeedRun>,
}

impl Point {
    /// Returns the measured MSE of every run that completed.
    pub fn completed(&self) -> Vec<f64> {
        self.runs.iter().filter_map(|r| r.mse).collect()
    }

    /// Returns the number of runs that did not complete.
    pub fn failures(&self) -> usize {
        self.runs.iter().filter(|r| r.failure.is_some()).count()
    }

    /// Summarizes the runs that completed, or returns `None` when none did.
    pub fn summary(&self) -> Option<Summary> {
        Summary::new(&self.completed())
    }
}

/// Runs every seed at one grid value.
///
/// # Errors
///
/// Returns a message if `value` does not produce a valid learner configuration.
/// Numerical failures during a run are recorded in the returned [`Point`].
pub fn measure(
    family: Family,
    protocol: &Protocol,
    value: f64,
    seeds: &[u64],
) -> Result<Point, String> {
    let learner = family.learner(value)?;
    let mut runs = Vec::with_capacity(seeds.len());
    for &seed in seeds {
        let result = run(&RunSpec {
            preset: protocol.preset,
            seed,
            warmup: protocol.warmup,
            measured: protocol.measured,
            learner,
        })?;
        runs.push(SeedRun {
            seed,
            mse: result.mse,
            failure: result
                .failure
                .map(|(sample, reason)| Failure { sample, reason }),
        });
    }
    Ok(Point { value, runs })
}

/// Returns the grid value with the lowest mean MSE among those whose every run
/// completed, or `None` if no value did.
///
/// A value with even one failure is not selectable: a mean over the survivors of a
/// diverging setting would understate its risk. Ties keep the earlier grid entry.
pub fn best(points: &[Point]) -> Option<&Point> {
    points
        .iter()
        .filter(|p| p.failures() == 0)
        .filter_map(|p| p.summary().map(|s| (p, s.mean())))
        .reduce(|lowest, candidate| {
            if candidate.1 < lowest.1 {
                candidate
            } else {
                lowest
            }
        })
        .map(|(point, _)| point)
}

/// Summarizes the per-seed differences `a - b` over the seeds where both runs completed.
///
/// The two points must list the same seeds in the same order, which [`measure`]
/// guarantees within one sweep. Pairing by seed removes the stream-to-stream variation
/// that dominates the standard error of either point on its own.
pub fn paired_difference(a: &Point, b: &Point) -> Option<Summary> {
    let differences: Vec<f64> = a
        .runs
        .iter()
        .zip(&b.runs)
        .filter(|(left, right)| left.seed == right.seed)
        .filter_map(|(left, right)| Some(left.mse? - right.mse?))
        .collect();
    Summary::new(&differences)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn point(value: f64, mses: &[Option<f64>]) -> Point {
        Point {
            value,
            runs: mses
                .iter()
                .enumerate()
                .map(|(i, &mse)| SeedRun {
                    seed: u64::try_from(i).expect("small index"),
                    mse,
                    failure: mse.is_none().then(|| Failure {
                        sample: 17,
                        reason: "weight 3 is not finite".to_owned(),
                    }),
                })
                .collect(),
        }
    }

    #[test]
    fn best_picks_the_lowest_complete_mean() {
        let points = [
            point(0.01, &[Some(4.0), Some(4.0)]),
            point(0.02, &[Some(2.0), Some(3.0)]),
            point(0.03, &[Some(3.0), Some(3.0)]),
        ];
        let chosen = best(&points).expect("one value completed");
        assert!((chosen.value - 0.02).abs() < 1e-12, "{chosen:?}");
    }

    #[test]
    fn best_skips_a_value_with_any_failure() {
        let points = [
            point(0.01, &[Some(4.0), Some(4.0)]),
            point(0.02, &[Some(0.5), None]),
        ];
        let chosen = best(&points).expect("one value completed");
        assert!((chosen.value - 0.01).abs() < 1e-12, "{chosen:?}");
        assert_eq!(points[1].failures(), 1);
        assert_eq!(points[1].completed(), vec![0.5]);
    }

    #[test]
    fn best_keeps_the_earlier_entry_on_a_tie() {
        let points = [point(0.01, &[Some(2.0)]), point(0.02, &[Some(2.0)])];
        let chosen = best(&points).expect("both completed");
        assert!((chosen.value - 0.01).abs() < 1e-12, "{chosen:?}");
    }

    #[test]
    fn best_of_nothing_selectable_is_none() {
        assert_eq!(best(&[]), None);
        assert_eq!(best(&[point(0.01, &[None])]), None);
    }

    #[test]
    fn paired_differences_use_only_seeds_both_completed() {
        let a = point(0.01, &[Some(4.0), Some(5.0), None]);
        let b = point(0.02, &[Some(3.0), Some(3.0), Some(1.0)]);
        let difference = paired_difference(&a, &b).expect("two shared seeds");
        assert_eq!(difference.count(), 2);
        assert!((difference.mean() - 1.5).abs() < 1e-12, "{difference:?}");
        assert_eq!(paired_difference(&a, &point(0.03, &[None])), None);
    }

    #[test]
    fn point_summarizes_only_completed_runs() {
        let p = point(0.05, &[Some(1.0), None, Some(3.0)]);
        let summary = p.summary().expect("two runs completed");
        assert_eq!(summary.count(), 2);
        assert!((summary.mean() - 2.0).abs() < 1e-12, "{summary:?}");
        assert_eq!(p.failures(), 1);
    }

    #[test]
    fn families_name_their_parameter_and_build_learners() {
        assert_eq!(Family::Lms.learner_id(), "lms");
        assert_eq!(Family::Lms.parameter(), "alpha");
        let idbd = Family::Idbd {
            initial_step_size: 0.05,
            bounds: Bounds::none(),
        };
        assert_eq!(idbd.learner_id(), "idbd");
        assert_eq!(idbd.parameter(), "theta");
        assert!(matches!(
            idbd.learner(0.0),
            Ok(LearnerSpec::Idbd(config)) if config.meta_step_size().abs() < f64::EPSILON
        ));
        assert!(Family::Lms.learner(0.0).is_err(), "alpha must be positive");
        assert!(idbd.learner(-1.0).is_err(), "theta must be nonnegative");
    }

    #[test]
    fn a_measured_point_records_one_run_per_seed() {
        let protocol = Protocol {
            preset: Preset::Sutton1992,
            warmup: 200,
            measured: 100,
        };
        let p = measure(Family::Lms, &protocol, 0.04, &[1, 2, 3]).expect("valid");
        assert_eq!(p.runs.len(), 3);
        assert_eq!(
            p.runs.iter().map(|r| r.seed).collect::<Vec<_>>(),
            vec![1, 2, 3]
        );
        assert_eq!(p.failures(), 0, "{p:?}");
    }
}
