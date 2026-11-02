//! A small, scripted IDBD example that explains one update at a time.

use idbd::{Config, Idbd, Update};

use crate::rng::Rng;
use crate::trace::{CoordinateChange, traced_update};

/// Inputs in the walkthrough: two that matter and one that does not.
pub const WALKTHROUGH_FEATURES: usize = 3;

/// True coefficients of the walkthrough task: `target = 2 x0 - x1 + 0 x2`.
pub const WALKTHROUGH_COEFFICIENTS: [f64; WALKTHROUGH_FEATURES] = [2.0, -1.0, 0.0];

/// Initial step size of the walkthrough learner.
pub const WALKTHROUGH_INITIAL_STEP_SIZE: f64 = 0.1;

/// Meta step size of the walkthrough learner. Larger than in Sutton's experiments so
/// that a single update visibly moves beta.
pub const WALKTHROUGH_META_STEP_SIZE: f64 = 0.1;

/// A three-input IDBD learner fed one seeded sample at a time.
#[derive(Debug, Clone)]
pub struct Walkthrough {
    learner: Idbd,
    rng: Rng,
    sample: u64,
    features: [f64; WALKTHROUGH_FEATURES],
    target: f64,
    report: Option<Update>,
    changes: [CoordinateChange; WALKTHROUGH_FEATURES],
    attempt_changes: [CoordinateChange; WALKTHROUGH_FEATURES],
}

impl Walkthrough {
    /// Starts a walkthrough at sample 0.
    pub fn new(seed: u64) -> Self {
        let config = Config::new(WALKTHROUGH_INITIAL_STEP_SIZE, WALKTHROUGH_META_STEP_SIZE)
            .expect("walkthrough constants are valid");
        Self {
            learner: Idbd::new(WALKTHROUGH_FEATURES, config).expect("three features"),
            rng: Rng::new(seed),
            sample: 0,
            features: [0.0; WALKTHROUGH_FEATURES],
            target: 0.0,
            report: None,
            changes: [CoordinateChange::default(); WALKTHROUGH_FEATURES],
            attempt_changes: [CoordinateChange::default(); WALKTHROUGH_FEATURES],
        }
    }

    /// Draws the next sample and lets the core learner learn from it.
    ///
    /// Every published view stays at the last completed sample if the update fails.
    ///
    /// # Errors
    ///
    /// Returns the core error; the walkthrough's bounded inputs make this unexpected.
    pub fn advance(&mut self) -> Result<&Update, idbd::Error> {
        let mut features = [0.0; WALKTHROUGH_FEATURES];
        for x in &mut features {
            // Rounded inputs keep the explanation readable.
            *x = (self.rng.standard_normal() * 10.0).round() / 10.0;
        }
        let target: f64 = WALKTHROUGH_COEFFICIENTS
            .iter()
            .zip(&features)
            .map(|(c, x)| c * x)
            .sum();
        // Traced into a scratch buffer so a rejected sample leaves the published
        // per-coordinate view on the last completed one.
        let report = traced_update(
            &mut self.learner,
            &features,
            target,
            &mut self.attempt_changes,
        )?;
        self.changes = self.attempt_changes;
        self.features = features;
        self.target = target;
        self.sample += 1;
        Ok(self.report.insert(report))
    }

    /// Returns the number of completed samples.
    pub fn sample(&self) -> u64 {
        self.sample
    }

    /// Returns the inputs of the last sample.
    pub fn features(&self) -> &[f64] {
        &self.features
    }

    /// Returns the target of the last sample.
    pub fn target(&self) -> f64 {
        self.target
    }

    /// Returns the core report of the last sample.
    pub fn report(&self) -> Option<&Update> {
        self.report.as_ref()
    }

    /// Returns each coordinate's change on the last sample.
    pub fn changes(&self) -> &[CoordinateChange] {
        &self.changes
    }

    /// Returns the learner.
    pub fn learner(&self) -> &Idbd {
        &self.learner
    }
}

#[cfg(test)]
#[expect(clippy::float_cmp, reason = "tests assert exact, deterministic values")]
mod tests {
    use super::*;

    #[test]
    fn samples_are_rounded_draws_and_the_target_follows_the_coefficients() {
        let mut stream = Rng::new(4);
        let mut walk = Walkthrough::new(4);
        walk.advance().expect("first");
        let expected: Vec<f64> = (0..WALKTHROUGH_FEATURES)
            .map(|_| (stream.standard_normal() * 10.0).round() / 10.0)
            .collect();
        assert_eq!(walk.features(), expected.as_slice());
        assert!(
            walk.features().iter().any(|x| x.abs() > 0.2),
            "the draws are not all rounded away: {:?}",
            walk.features()
        );
        let inputs: Vec<f64> = walk.changes().iter().map(|c| c.input).collect();
        assert_eq!(
            inputs.as_slice(),
            walk.features(),
            "the coordinate view describes the completed sample"
        );
        let x = walk.features();
        assert_eq!(walk.target(), 2.0 * x[0] - x[1]);
        assert!(
            walk.target() != 2.0 * x[0] + x[1],
            "the second coefficient is negative: {x:?}"
        );
    }

    #[test]
    fn first_update_leaves_beta_unchanged_then_beta_moves() {
        let mut walk = Walkthrough::new(1);
        walk.advance().expect("first");
        assert!(
            walk.changes().iter().all(|c| c.beta_change() == 0.0),
            "zero old traces"
        );
        walk.advance().expect("second");
        assert!(
            walk.changes().iter().any(|c| c.beta_change() != 0.0),
            "{:?}",
            walk.changes()
        );
        assert_eq!(walk.sample(), 2);
        let report = walk.report().copied().expect("report");
        assert!((report.error - (walk.target() - report.prediction)).abs() < 1e-12);
    }
}
