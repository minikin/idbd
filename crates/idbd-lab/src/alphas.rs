//! Step-size trajectories from a long IDBD run, for Sutton (1992), Experiment 2.
//!
//! The run records how the step sizes of the relevant and irrelevant inputs separate
//! over time. Relevance is read from the task at each checkpoint, so the recording also
//! works on the changing-relevance extension.

use idbd::{Config, Idbd};

use crate::stats::Summary;
use crate::sweep::Failure;
use crate::world::{GROUP_SIZE, NUM_FEATURES, Preset, World};

/// Step sizes of the relevant and irrelevant inputs at one sample index.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Checkpoint {
    /// Number of samples processed, counting from one.
    pub sample: u64,
    /// Mean step size of the currently relevant inputs.
    pub relevant_mean: f64,
    /// Smallest step size among the currently relevant inputs.
    pub relevant_min: f64,
    /// Largest step size among the currently relevant inputs.
    pub relevant_max: f64,
    /// Mean step size of the irrelevant inputs.
    pub irrelevant_mean: f64,
    /// Largest step size among the irrelevant inputs.
    pub irrelevant_max: f64,
}

/// A recorded long run.
#[derive(Debug, Clone, PartialEq)]
pub struct Trajectory {
    /// The task seed.
    pub seed: u64,
    /// One entry every `interval` samples, plus the final sample.
    pub checkpoints: Vec<Checkpoint>,
    /// Step size of every input after the last completed sample.
    pub final_step_sizes: Vec<f64>,
    /// Index of the relevant group after the last completed sample.
    pub relevant_group: usize,
    /// Mean squared pre-update error over the last `interval` completed samples.
    pub tail_mse: Option<f64>,
    /// The failure that ended the run, if any.
    pub failure: Option<Failure>,
}

impl Trajectory {
    /// Summarizes the final step sizes of the relevant inputs.
    pub fn final_relevant(&self) -> Option<Summary> {
        Summary::new(&self.split().0)
    }

    /// Summarizes the final step sizes of the irrelevant inputs.
    pub fn final_irrelevant(&self) -> Option<Summary> {
        Summary::new(&self.split().1)
    }

    /// Returns the largest final step size among the irrelevant inputs.
    pub fn final_irrelevant_max(&self) -> Option<f64> {
        self.split().1.into_iter().reduce(f64::max)
    }

    fn split(&self) -> (Vec<f64>, Vec<f64>) {
        split_by_relevance(self.final_step_sizes.iter().copied(), self.relevant_group)
    }
}

fn split_by_relevance(step_sizes: impl Iterator<Item = f64>, group: usize) -> (Vec<f64>, Vec<f64>) {
    let relevant_indices = relevant_range(group);
    let mut relevant = Vec::with_capacity(GROUP_SIZE);
    let mut irrelevant = Vec::with_capacity(NUM_FEATURES - GROUP_SIZE);
    for (index, alpha) in step_sizes.enumerate() {
        if relevant_indices.contains(&index) {
            relevant.push(alpha);
        } else {
            irrelevant.push(alpha);
        }
    }
    (relevant, irrelevant)
}

const fn relevant_range(group: usize) -> std::ops::Range<usize> {
    let start = group * GROUP_SIZE;
    start..start + GROUP_SIZE
}

/// Runs IDBD for `samples` examples on a seeded task, recording step sizes every
/// `interval` samples.
///
/// # Errors
///
/// Returns a message if `config` is invalid for the task's feature count. A numerical
/// failure during the run is recorded in [`Trajectory::failure`] instead.
///
/// # Panics
///
/// Panics if `interval` is zero.
#[expect(
    clippy::panic_in_result_fn,
    reason = "a zero checkpoint interval is a caller bug, not a learning error"
)]
pub fn record(
    preset: Preset,
    seed: u64,
    samples: u64,
    interval: u64,
    config: Config,
) -> Result<Trajectory, String> {
    assert!(interval > 0, "checkpoint interval must be positive");
    let mut learner = Idbd::new(NUM_FEATURES, config).map_err(|e| e.to_string())?;
    let mut world = World::new(preset, seed);
    let mut x = [0.0; NUM_FEATURES];
    let mut events = Vec::new();
    let mut checkpoints = Vec::new();
    let mut failure = None;
    let mut tail_sum = 0.0;
    let mut tail_count = 0_u64;
    let tail_start = samples.saturating_sub(interval);
    for t in 1..=samples {
        let target = world.next_sample(&mut x, &mut events);
        match learner.update(&x, target) {
            Ok(report) => {
                if t > tail_start {
                    tail_sum += report.error * report.error;
                    tail_count += 1;
                }
            }
            Err(error) => {
                failure = Some(Failure {
                    sample: t,
                    reason: error.to_string(),
                });
                break;
            }
        }
        if t % interval == 0 || t == samples {
            checkpoints.push(checkpoint(t, &learner, world.relevant_group()));
        }
    }
    let tail_mse = (failure.is_none() && tail_count > 0)
        .then(|| tail_sum / crate::stats::exact_f64(tail_count));
    Ok(Trajectory {
        seed,
        checkpoints,
        final_step_sizes: learner.step_sizes().collect(),
        relevant_group: world.relevant_group(),
        tail_mse,
        failure,
    })
}

fn checkpoint(sample: u64, learner: &Idbd, group: usize) -> Checkpoint {
    let (relevant_values, irrelevant_values) = split_by_relevance(learner.step_sizes(), group);
    let mean = |values: &[f64]| {
        Summary::new(values)
            .expect("every group has at least one input")
            .mean()
    };
    Checkpoint {
        sample,
        relevant_mean: mean(&relevant_values),
        relevant_min: relevant_values
            .iter()
            .copied()
            .reduce(f64::min)
            .expect("the relevant group is not empty"),
        relevant_max: relevant_values
            .iter()
            .copied()
            .reduce(f64::max)
            .expect("the relevant group is not empty"),
        irrelevant_mean: mean(&irrelevant_values),
        irrelevant_max: irrelevant_values
            .iter()
            .copied()
            .reduce(f64::max)
            .expect("fifteen inputs are irrelevant"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checkpoints_land_on_the_interval_and_the_end() {
        let config = Config::new(0.05, 0.001).expect("valid");
        let t = record(Preset::Sutton1992, 1, 250, 100, config).expect("valid");
        let samples: Vec<u64> = t.checkpoints.iter().map(|c| c.sample).collect();
        assert_eq!(samples, vec![100, 200, 250]);
        assert_eq!(t.final_step_sizes.len(), NUM_FEATURES);
        assert_eq!(t.relevant_group, 0);
        assert!(t.failure.is_none(), "{:?}", t.failure);
        assert!(t.tail_mse.is_some_and(f64::is_finite), "{t:?}");
    }

    #[test]
    fn relevant_and_irrelevant_inputs_are_summarized_separately() {
        let config = Config::new(0.05, 0.001).expect("valid");
        let t = record(Preset::Sutton1992, 2, 2_000, 1_000, config).expect("valid");
        let relevant = t.final_relevant().expect("five relevant inputs");
        let irrelevant = t.final_irrelevant().expect("fifteen irrelevant inputs");
        assert_eq!(relevant.count(), GROUP_SIZE);
        assert_eq!(irrelevant.count(), NUM_FEATURES - GROUP_SIZE);
        assert!(
            relevant.mean() > irrelevant.mean(),
            "relevant {relevant:?} vs irrelevant {irrelevant:?}"
        );
        assert!(
            t.final_irrelevant_max().is_some_and(f64::is_finite),
            "{t:?}"
        );
    }

    fn trajectory(relevant_group: usize) -> Trajectory {
        let step_sizes = (0..NUM_FEATURES)
            .map(|i| f64::from(u8::try_from(i).expect("twenty inputs")))
            .collect();
        Trajectory {
            seed: 0,
            checkpoints: Vec::new(),
            final_step_sizes: step_sizes,
            relevant_group,
            tail_mse: None,
            failure: None,
        }
    }

    #[test]
    fn the_split_follows_the_group_the_task_reports() {
        let t = trajectory(1);
        let relevant = t.final_relevant().expect("five relevant inputs");
        assert_eq!(relevant.count(), GROUP_SIZE);
        // Step sizes are 0..20, so group 1 holds 5..10.
        assert!((relevant.mean() - 7.0).abs() < 1e-12, "{relevant:?}");
        let irrelevant = t.final_irrelevant().expect("fifteen irrelevant inputs");
        assert_eq!(irrelevant.count(), NUM_FEATURES - GROUP_SIZE);
        // Indices 0..5 and 10..20 sum to 155.
        assert!(
            (irrelevant.mean() - 155.0 / 15.0).abs() < 1e-12,
            "{irrelevant:?}"
        );
        let max = t.final_irrelevant_max().expect("fifteen irrelevant inputs");
        assert!(
            (max - 19.0).abs() < 1e-12,
            "largest irrelevant step size {max}"
        );
    }

    #[test]
    fn tail_mse_averages_the_last_interval_of_samples() {
        let config = Config::new(0.05, 0.001).expect("valid");
        let recorded = record(Preset::Sutton1992, 4, 120, 50, config).expect("valid");

        let mut learner = Idbd::new(NUM_FEATURES, config).expect("valid");
        let mut world = World::new(Preset::Sutton1992, 4);
        let mut x = [0.0; NUM_FEATURES];
        let mut events = Vec::new();
        let mut tail = 0.0;
        let mut head = 0.0;
        let mut counted = 0_u32;
        for sample in 1..=120_u64 {
            let target = world.next_sample(&mut x, &mut events);
            let error = learner.update(&x, target).expect("finite").error;
            if sample > 70 {
                tail += error * error;
                counted += 1;
            } else {
                head += error * error;
            }
        }
        assert_eq!(counted, 50, "the tail is the last interval of samples");
        assert!(head > 0.0, "the earlier samples carry error too");
        let measured = recorded.tail_mse.expect("the run completed");
        assert_eq!(
            measured.to_bits(),
            (tail / 50.0).to_bits(),
            "{measured} vs {}",
            tail / 50.0
        );
    }

    #[test]
    fn a_run_without_samples_has_no_tail() {
        let config = Config::new(0.05, 0.001).expect("valid");
        let t = record(Preset::Sutton1992, 1, 0, 10, config).expect("valid");
        assert!(t.checkpoints.is_empty(), "{t:?}");
        assert!(t.failure.is_none(), "{t:?}");
        assert_eq!(t.tail_mse, None, "no sample was measured");
    }

    #[test]
    fn a_failed_run_has_no_tail_mse() {
        let config = Config::new(0.5, 1e6).expect("valid");
        let t = record(Preset::Sutton1992, 1, 2_000, 2_000, config).expect("valid");
        let failure = t.failure.clone().expect("a huge meta step size diverges");
        assert!(
            failure.sample > 1,
            "samples completed before the failure: {failure:?}"
        );
        assert_eq!(t.tail_mse, None, "a partial tail is not a mean");
    }

    #[test]
    fn runs_are_reproducible() {
        let config = Config::new(0.05, 0.001).expect("valid");
        let first = record(Preset::Sutton1992, 7, 500, 250, config).expect("valid");
        let second = record(Preset::Sutton1992, 7, 500, 250, config).expect("valid");
        assert_eq!(first, second);
    }

    #[test]
    #[should_panic(expected = "interval must be positive")]
    fn a_zero_interval_is_rejected() {
        let config = Config::new(0.05, 0.001).expect("valid");
        let _ = record(Preset::Sutton1992, 1, 10, 0, config);
    }
}
