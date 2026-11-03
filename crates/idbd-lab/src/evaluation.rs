//! The E01 evaluation protocol, shared by the `evaluate` and `autostep-eval` binaries.
//!
//! Each learner family's one free parameter is selected on [`TUNING_SEEDS`], then the
//! selected value is measured on [`EVALUATION_SEEDS`], which no tuning run uses.

use crate::sweep::{Family, Point, Protocol, best, measure};
use crate::task::Task;
use crate::world::Preset;

/// Fixed LMS step sizes offered to the tuning procedure.
pub const LMS_GRID: [f64; 11] = [
    0.001, 0.002, 0.005, 0.01, 0.02, 0.03, 0.04, 0.05, 0.06, 0.08, 0.10,
];

/// IDBD meta step sizes offered to the tuning procedure.
pub const IDBD_GRID: [f64; 9] = [0.0, 0.0005, 0.001, 0.002, 0.004, 0.006, 0.008, 0.010, 0.015];

/// Seeds used only to select a parameter. No evaluation seed appears here.
pub const TUNING_SEEDS: [u64; 5] = [1001, 1002, 1003, 1004, 1005];

/// Seeds used only to report results. Every learner sees these same streams.
pub const EVALUATION_SEEDS: [u64; 10] = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10];

/// Starting step size of every IDBD input.
pub const IDBD_INITIAL_STEP_SIZE: f64 = 0.05;

/// Samples processed before measurement starts.
pub const WARMUP: u64 = 20_000;

/// Samples whose squared pre-update error is averaged.
pub const MEASURED: u64 = 10_000;

/// The four evaluation tasks, in report order.
pub const TASKS: [Preset; 4] = [
    Preset::Sutton1992,
    Preset::Stationary,
    Preset::Drifting,
    Preset::ChangingRelevance,
];

/// The measurement protocol for one task.
pub const fn protocol(task: Task) -> Protocol {
    Protocol {
        task,
        warmup: WARMUP,
        measured: MEASURED,
    }
}

/// One family's tuning sweep, selected value and evaluation on one task.
#[derive(Debug, Clone, PartialEq)]
pub struct Tuned {
    /// The learner family.
    pub family: Family,
    /// One point per grid value, measured on the tuning seeds.
    pub tuning: Vec<Point>,
    /// The selected grid value.
    pub selected: f64,
    /// The selected value measured on the evaluation seeds.
    pub evaluation: Point,
}

/// Tunes `family` over `grid` on the tuning seeds, then measures the selected value on
/// the evaluation seeds.
///
/// # Errors
///
/// Returns a message if a grid value is not a valid configuration, or if no grid value
/// completed every tuning seed.
pub fn tune_and_evaluate(family: Family, grid: &[f64], task: Task) -> Result<Tuned, String> {
    let protocol = protocol(task);
    let mut tuning = Vec::with_capacity(grid.len());
    for &value in grid {
        tuning.push(measure(family, &protocol, value, &TUNING_SEEDS)?);
    }
    let selected = best(&tuning)
        .ok_or_else(|| {
            format!(
                "no {} grid value completed every tuning seed on {}",
                family.learner_id(),
                task.id()
            )
        })?
        .value;
    let evaluation = measure(family, &protocol, selected, &EVALUATION_SEEDS)?;
    Ok(Tuned {
        family,
        tuning,
        selected,
        evaluation,
    })
}

#[cfg(test)]
#[expect(clippy::float_cmp, reason = "tests assert exact, deterministic values")]
mod tests {
    use super::*;

    #[test]
    fn tuning_and_evaluation_seeds_are_disjoint() {
        assert!(TUNING_SEEDS.iter().all(|s| !EVALUATION_SEEDS.contains(s)));
    }

    #[test]
    fn tune_and_evaluate_selects_the_best_completed_value() {
        let tuned =
            tune_and_evaluate(Family::Lms, &[0.03, 5.0], Preset::Stationary.into()).expect("valid");
        assert_eq!(tuned.tuning.len(), 2);
        assert_eq!(
            tuned.tuning[1].failures(),
            TUNING_SEEDS.len(),
            "alpha 5 diverges"
        );
        assert_eq!(tuned.selected, 0.03);
        assert_eq!(tuned.evaluation.value, 0.03);
        let seeds: Vec<u64> = tuned.evaluation.runs.iter().map(|r| r.seed).collect();
        assert_eq!(seeds, EVALUATION_SEEDS);
    }

    #[test]
    fn a_grid_without_a_completed_value_is_an_error() {
        let message = tune_and_evaluate(Family::Lms, &[5.0], Preset::Stationary.into())
            .expect_err("every run diverges");
        assert_eq!(
            message,
            "no lms grid value completed every tuning seed on stationary"
        );
    }
}
