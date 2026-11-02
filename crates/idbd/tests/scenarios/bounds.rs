//! Rule: Bounds are explicit and their activity is observable.

use idbd::{Bounds, Config, Error, Idbd, Stage};

use crate::support::{assert_close, config, restore, safety_config};

/// A one-feature learner whose next update with features `[1.0]` and target `1.0` has a
/// raw beta increment of exactly `increment`: theta 1, error 1, x 1 and trace
/// `increment`.
fn primed(config: Config, old_beta: f64, increment: f64) -> Idbd {
    restore(config, &[0.0], &[old_beta], &[increment])
}

#[test]
fn c12_apply_the_optional_safety_bounds_in_the_specified_order() {
    let cases = [
        (-5.0, 3.0, -3.0, 1),
        (-5.0, -3.0, -7.0, 1),
        (-9.0, -3.0, -10.0, 1),
        (-10.0, -1.0, -10.0, 1),
        (-9.0, -1.0, -10.0, 0),
        (-5.0, 2.0, -3.0, 0),
    ];
    for (old_beta, increment, final_beta, count) in cases {
        let mut learner = primed(safety_config(0.05, 1.0), old_beta, increment);
        let report = learner.update(&[1.0], 1.0).expect("successful update");
        let case = format!("old {old_beta}, increment {increment}");
        assert_close(learner.log_step_sizes()[0], final_beta, &case);
        assert_eq!(report.bounded_features, count, "{case}");
    }
}

#[test]
fn c13_apply_an_optional_beta_ceiling() {
    let bounds = Bounds::none().with_max_beta(0.0).expect("valid ceiling");
    let config = config(0.05, 1.0).with_bounds(bounds).expect("valid config");
    let mut learner = primed(config, -1.0, 2.0);

    let report = learner.update(&[1.0], 1.0).expect("successful update");

    assert_eq!(learner.log_step_sizes(), &[0.0]);
    assert_eq!(learner.step_sizes().next(), Some(1.0));
    assert_eq!(report.bounded_features, 1);
}

#[test]
fn c14_preserve_the_original_equations_when_bounds_are_disabled() {
    let weights = [0.0, 0.0];
    let betas = [-4.0, -4.0];
    let traces = [3.0, 0.5];
    let mut unbounded = restore(config(0.05, 1.0), &weights, &betas, &traces);
    let mut bounded = restore(safety_config(0.05, 1.0), &weights, &betas, &traces);

    let free = unbounded
        .update(&[1.0, 1.0], 1.0)
        .expect("unbounded update");
    let safe = bounded.update(&[1.0, 1.0], 1.0).expect("bounded update");

    assert_close(unbounded.log_step_sizes()[0], -1.0, "full increment of 3");
    assert_close(bounded.log_step_sizes()[0], -2.0, "increment limited to 2");
    assert_eq!(
        unbounded.log_step_sizes()[1].to_bits(),
        bounded.log_step_sizes()[1].to_bits(),
        "an increment inside the limit is unaffected"
    );
    assert_eq!(free.bounded_features, 0);
    assert_eq!(safe.bounded_features, 1);

    for learner in [&mut unbounded, &mut bounded] {
        assert_eq!(
            learner.update(&[1.0, f64::NAN], 1.0),
            Err(Error::NonFiniteFeature { index: 1 })
        );
        let mut overflowing = restore(*learner.config(), &[0.0], &[-4.0], &[1e300]);
        assert_eq!(
            overflowing.update(&[1.0], 1e10),
            Err(Error::Numerical {
                stage: Stage::MetaStep,
                coordinate: Some(0)
            })
        );
    }
}
