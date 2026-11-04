//! C25: checkpoints through serde. Runs only with the `serde` feature.

#![cfg(feature = "serde")]

use idbd::autostep::{self, Autostep};
use idbd::{Bounds, Config, Idbd, State};

fn trained() -> Idbd {
    let config = Config::new(0.05, 0.01)
        .and_then(|c| c.with_bounds(Bounds::sutton_safety()))
        .expect("valid config");
    let mut learner = Idbd::new(3, config).expect("valid learner");
    for t in 0..50_u32 {
        let x = f64::from(t);
        let features = [(x * 0.37).sin(), (x * 0.11).cos(), 1.0];
        learner
            .update(&features, 2.0 * features[0] - features[1])
            .expect("finite sample");
    }
    learner
}

fn bits(state: &State) -> Vec<u64> {
    state
        .weights
        .iter()
        .chain(&state.betas)
        .chain(&state.traces)
        .map(|v| v.to_bits())
        .collect()
}

#[test]
fn c25_a_learner_round_trips_bit_for_bit_and_keeps_learning_the_same_way() {
    let mut original = trained();
    let json = serde_json::to_string(&original).expect("serializes");
    let mut restored: Idbd = serde_json::from_str(&json).expect("deserializes");

    assert_eq!(restored.config(), original.config());
    assert_eq!(bits(&restored.snapshot()), bits(&original.snapshot()));
    for features in [[0.3, -1.2, 1.0], [2.0, 0.5, 1.0]] {
        let a = original.update(&features, 0.7).expect("finite");
        let b = restored.update(&features, 0.7).expect("finite");
        assert_eq!(a.error.to_bits(), b.error.to_bits(), "same report");
    }
    assert_eq!(bits(&restored.snapshot()), bits(&original.snapshot()));
}

#[test]
fn c25_a_learner_and_its_snapshot_serialize_the_same_way() {
    let learner = trained();
    let from_learner = serde_json::to_value(&learner).expect("serializes");
    let from_state = serde_json::to_value(learner.snapshot()).expect("serializes");
    assert_eq!(from_learner, from_state);
    // Sorted, so the check does not depend on serde_json's map ordering features.
    let mut keys: Vec<&String> = from_learner
        .as_object()
        .expect("an object")
        .keys()
        .collect();
    keys.sort();
    assert_eq!(keys, ["betas", "config", "traces", "weights"]);
}

#[test]
fn c25_config_uses_named_fields_and_omitted_bounds_mean_none() {
    let config = Config::new(0.05, 0.01)
        .and_then(|c| c.with_bounds(Bounds::sutton_safety()))
        .expect("valid config");
    let value = serde_json::to_value(config).expect("serializes");
    assert_eq!(
        value,
        serde_json::json!({
            "initial_step_size": 0.05,
            "meta_step_size": 0.01,
            "bounds": { "min_beta": -10.0, "max_beta": null, "max_beta_increment": 2.0 }
        })
    );

    let plain: Config = serde_json::from_str(r#"{"initial_step_size":0.1,"meta_step_size":0.0}"#)
        .expect("bounds are optional");
    assert_eq!(plain, Config::new(0.1, 0.0).expect("valid config"));
    assert!(plain.bounds().is_none());
}

#[test]
fn c25_invalid_config_and_bounds_are_rejected_with_the_constructor_error() {
    let cases = [
        (
            r#"{"initial_step_size":-1.0,"meta_step_size":0.01}"#,
            Config::new(-1.0, 0.01).expect_err("invalid").to_string(),
        ),
        (
            r#"{"initial_step_size":0.1,"meta_step_size":-0.5}"#,
            Config::new(0.1, -0.5).expect_err("invalid").to_string(),
        ),
        (
            r#"{"initial_step_size":0.1,"meta_step_size":0.01,"bounds":{"min_beta":0.0}}"#,
            Config::new(0.1, 0.01)
                .expect("valid")
                .with_bounds(Bounds::none().with_min_beta(0.0).expect("valid floor"))
                .expect_err("ln 0.1 is below the floor")
                .to_string(),
        ),
        (
            r#"{"initial_step_size":0.1,"meta_step_size":0.01,"bounds":{"min_beta":1.0,"max_beta":0.0}}"#,
            Bounds::none()
                .with_min_beta(1.0)
                .and_then(|b| b.with_max_beta(0.0))
                .expect_err("ceiling below floor")
                .to_string(),
        ),
    ];
    for (json, expected) in cases {
        let message = serde_json::from_str::<Config>(json)
            .expect_err(json)
            .to_string();
        assert!(message.starts_with(&expected), "{json}: {message}");
    }
}

#[test]
fn c25_unknown_fields_are_rejected() {
    let typo = r#"{"initial_step_size":0.1,"meta_stepsize":0.01}"#;
    let message = serde_json::from_str::<Config>(typo)
        .expect_err("typo")
        .to_string();
    assert!(
        message.contains("unknown field `meta_stepsize`"),
        "{message}"
    );
}

#[test]
fn c25_an_invalid_checkpoint_deserializes_neither_as_state_nor_as_a_learner() {
    let mut state = trained().snapshot();
    state.betas[1] = -20.0;
    let json = serde_json::to_string(&state).expect("serializes");

    let expected = Idbd::from_state(state).expect_err("beta below the floor");
    for message in [
        serde_json::from_str::<State>(&json)
            .expect_err("State is validated")
            .to_string(),
        serde_json::from_str::<Idbd>(&json)
            .expect_err("Idbd is validated")
            .to_string(),
    ] {
        assert!(message.starts_with(&expected.to_string()), "{message}");
    }

    let short = serde_json::json!({
        "config": serde_json::to_value(trained().config()).expect("serializes"),
        "weights": [0.0], "betas": [], "traces": [0.0],
    });
    let message = serde_json::from_value::<State>(short)
        .expect_err("lengths differ")
        .to_string();
    assert!(
        message.starts_with("state vectors differ in length"),
        "{message}"
    );
}

#[test]
fn c25_bounds_round_trip_on_their_own() {
    let bounds = Bounds::sutton_safety()
        .with_max_beta(0.0)
        .expect("ceiling above floor");
    let json = serde_json::to_string(&bounds).expect("serializes");
    assert_eq!(
        serde_json::from_str::<Bounds>(&json).expect("deserializes"),
        bounds
    );
    let message = serde_json::from_str::<Bounds>(r#"{"max_beta_increment":-1.0}"#)
        .expect_err("negative increment")
        .to_string();
    let expected = Bounds::none()
        .with_max_beta_increment(-1.0)
        .expect_err("negative increment")
        .to_string();
    assert!(message.starts_with(&expected), "{message}");
}

#[test]
fn c25_autostep_round_trips_bit_for_bit_and_rejects_invalid_input() {
    let config = autostep::Config::new(0.1)
        .and_then(|c| c.with_time_scale(100.0))
        .expect("valid config");
    let mut original = Autostep::new(2, config).expect("valid learner");
    for t in 0..30_u32 {
        let x = f64::from(t).sin();
        original.update(&[x, 1.0], 3.0 * x).expect("finite");
    }
    let json = serde_json::to_string(&original).expect("serializes");
    assert_eq!(
        serde_json::to_value(&original).expect("serializes"),
        serde_json::to_value(original.snapshot()).expect("serializes")
    );
    let mut restored: Autostep = serde_json::from_str(&json).expect("deserializes");
    assert_eq!(restored.snapshot(), original.snapshot());
    assert_eq!(
        original.update(&[0.4, 1.0], 1.2),
        restored.update(&[0.4, 1.0], 1.2)
    );
    assert_eq!(
        serde_json::to_value(config).expect("serializes"),
        serde_json::json!({ "initial_step_size": 0.1, "meta_step_size": 0.01, "time_scale": 100.0 })
    );

    let mut state = original.snapshot();
    state.normalizers[0] = -1.0;
    let invalid = serde_json::to_string(&state).expect("serializes");
    let expected = Autostep::from_state(state).expect_err("negative normalizer");
    for message in [
        serde_json::from_str::<autostep::State>(&invalid)
            .expect_err("State is validated")
            .to_string(),
        serde_json::from_str::<Autostep>(&invalid)
            .expect_err("Autostep is validated")
            .to_string(),
    ] {
        assert!(message.starts_with(&expected.to_string()), "{message}");
    }

    for (json, expected) in [
        (
            r#"{"initial_step_size":0.0,"meta_step_size":0.01,"time_scale":10.0}"#,
            autostep::Config::new(0.0).expect_err("invalid").to_string(),
        ),
        (
            r#"{"initial_step_size":0.1,"meta_step_size":-1.0,"time_scale":10.0}"#,
            config
                .with_meta_step_size(-1.0)
                .expect_err("invalid")
                .to_string(),
        ),
        (
            r#"{"initial_step_size":0.1,"meta_step_size":0.01,"time_scale":0.5}"#,
            config
                .with_time_scale(0.5)
                .expect_err("invalid")
                .to_string(),
        ),
    ] {
        let message = serde_json::from_str::<autostep::Config>(json)
            .expect_err(json)
            .to_string();
        assert!(message.starts_with(&expected), "{json}: {message}");
    }
}
