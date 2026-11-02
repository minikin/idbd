//! Records how one IDBD update changed each coordinate, using only values read from
//! the core learner before and after its own update.

use idbd::{Error, Idbd, Update};

/// One coordinate's committed values before and after an update.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct CoordinateChange {
    /// The input value `x_i`.
    pub input: f64,
    /// Weight before the update.
    pub old_weight: f64,
    /// Weight after the update.
    pub new_weight: f64,
    /// Log step size before the update.
    pub old_beta: f64,
    /// Log step size after the update.
    pub new_beta: f64,
    /// Trace before the update.
    pub old_trace: f64,
    /// Trace after the update.
    pub new_trace: f64,
}

impl CoordinateChange {
    /// Step size before the update.
    pub fn old_alpha(&self) -> f64 {
        self.old_beta.exp()
    }

    /// Step size after the update, used for this update's weight and trace changes.
    pub fn new_alpha(&self) -> f64 {
        self.new_beta.exp()
    }

    /// Change in beta.
    pub fn beta_change(&self) -> f64 {
        self.new_beta - self.old_beta
    }

    /// Change in the weight.
    pub fn weight_change(&self) -> f64 {
        self.new_weight - self.old_weight
    }
}

/// Updates `learner` and records each coordinate's change into `changes`.
///
/// `changes` must hold one entry per feature. On error the learner is unchanged and
/// `changes` describes the attempted inputs with equal old and new values, so a caller
/// that publishes a per-coordinate view must pass a scratch buffer and copy it over only
/// on success.
///
/// # Errors
///
/// Returns the learner's own [`Error`].
///
/// # Panics
///
/// Panics if `changes` and `features` differ in length from the learner.
#[expect(
    clippy::panic_in_result_fn,
    reason = "a wrong buffer length is a caller bug, not a learning error"
)]
pub fn traced_update(
    learner: &mut Idbd,
    features: &[f64],
    target: f64,
    changes: &mut [CoordinateChange],
) -> Result<Update, Error> {
    assert_eq!(
        changes.len(),
        learner.num_features(),
        "one change per feature"
    );
    let before = learner
        .weights()
        .iter()
        .zip(learner.log_step_sizes())
        .zip(learner.traces());
    for (change, ((&w, &beta), &h)) in changes.iter_mut().zip(before) {
        *change = CoordinateChange {
            old_weight: w,
            new_weight: w,
            old_beta: beta,
            new_beta: beta,
            old_trace: h,
            new_trace: h,
            input: 0.0,
        };
    }
    for (change, &x) in changes.iter_mut().zip(features) {
        change.input = x;
    }
    let report = learner.update(features, target)?;
    let after = learner
        .weights()
        .iter()
        .zip(learner.log_step_sizes())
        .zip(learner.traces());
    for (change, ((&w, &beta), &h)) in changes.iter_mut().zip(after) {
        change.new_weight = w;
        change.new_beta = beta;
        change.new_trace = h;
    }
    Ok(report)
}

#[cfg(test)]
#[expect(clippy::float_cmp, reason = "tests assert exact, deterministic values")]
mod tests {
    use super::*;
    use idbd::Config;

    #[test]
    fn records_the_c07_update() {
        let mut learner = Idbd::new(2, Config::new(0.1, 0.01).expect("valid")).expect("valid");
        let mut changes = [CoordinateChange::default(); 2];
        traced_update(&mut learner, &[1.0, 2.0], 1.0, &mut changes).expect("C06");
        let report = traced_update(&mut learner, &[2.0, -1.0], 0.5, &mut changes).expect("C07");
        assert!(report.error > 0.0, "{report:?}");
        let c = changes[0];
        assert_eq!(c.input, 2.0);
        assert!(
            (c.old_alpha() - 0.1).abs() < 1e-15,
            "the configured initial step size: {c:?}"
        );
        assert!(
            (c.old_trace - 0.1).abs() < 1e-15,
            "old trace from C06: {c:?}"
        );
        assert!((c.beta_change() - 0.001).abs() < 1e-12, "{c:?}");
        assert!(
            (c.new_alpha() - 0.100_100_050_016_670_84).abs() < 1e-12,
            "{c:?}"
        );
        assert!(
            (c.weight_change() - 0.100_100_050_016_670_84).abs() < 1e-12,
            "{c:?}"
        );
        assert_eq!(c.new_weight, learner.weights()[0]);
        assert_eq!(c.new_trace, learner.traces()[0]);
        assert!(changes[1].beta_change() < 0.0, "{:?}", changes[1]);
    }

    #[test]
    fn failed_update_reports_no_change() {
        let mut learner = Idbd::new(1, Config::new(0.1, 0.01).expect("valid")).expect("valid");
        let mut changes = [CoordinateChange::default(); 1];
        assert!(traced_update(&mut learner, &[1.0], f64::NAN, &mut changes).is_err());
        assert_eq!(changes[0].weight_change(), 0.0);
        assert_eq!(changes[0].beta_change(), 0.0);
    }
}
