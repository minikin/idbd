//! Optional `serde` support, enabled by the `serde` feature.
//!
//! [`Bounds`] and [`Config`] deserialize through their validating constructors, and
//! [`Idbd`] deserializes through [`Idbd::from_state`], so invalid input is an error, never
//! a learner that breaks an invariant. [`State`] stays unvalidated, as with
//! [`State::new`].

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::{Bounds, Config, Idbd, State};

#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BoundsRepr {
    #[serde(default)]
    min_beta: Option<f64>,
    #[serde(default)]
    max_beta: Option<f64>,
    #[serde(default)]
    max_beta_increment: Option<f64>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ConfigRepr {
    initial_step_size: f64,
    meta_step_size: f64,
    #[serde(default)]
    bounds: BoundsRepr,
}

/// Borrows a learner's parts so that serializing an [`Idbd`] copies nothing.
#[derive(Serialize)]
struct StateRef<'state> {
    config: &'state Config,
    weights: &'state [f64],
    betas: &'state [f64],
    traces: &'state [f64],
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StateRepr {
    config: Config,
    weights: Vec<f64>,
    betas: Vec<f64>,
    traces: Vec<f64>,
}

impl From<&Bounds> for BoundsRepr {
    fn from(bounds: &Bounds) -> Self {
        Self {
            min_beta: bounds.min_beta(),
            max_beta: bounds.max_beta(),
            max_beta_increment: bounds.max_beta_increment(),
        }
    }
}

impl BoundsRepr {
    fn validate(self) -> Result<Bounds, crate::ConfigError> {
        let mut bounds = Bounds::none();
        if let Some(value) = self.min_beta {
            bounds = bounds.with_min_beta(value)?;
        }
        if let Some(value) = self.max_beta {
            bounds = bounds.with_max_beta(value)?;
        }
        if let Some(value) = self.max_beta_increment {
            bounds = bounds.with_max_beta_increment(value)?;
        }
        Ok(bounds)
    }
}

impl Serialize for Bounds {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        BoundsRepr::from(self).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Bounds {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        BoundsRepr::deserialize(deserializer)?
            .validate()
            .map_err(D::Error::custom)
    }
}

impl Serialize for Config {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        ConfigRepr {
            initial_step_size: self.initial_step_size(),
            meta_step_size: self.meta_step_size(),
            bounds: BoundsRepr::from(self.bounds()),
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Config {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let repr = ConfigRepr::deserialize(deserializer)?;
        let bounds = repr.bounds.validate().map_err(D::Error::custom)?;
        Config::new(repr.initial_step_size, repr.meta_step_size)
            .and_then(|config| config.with_bounds(bounds))
            .map_err(D::Error::custom)
    }
}

impl Serialize for State {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        StateRef {
            config: &self.config,
            weights: &self.weights,
            betas: &self.betas,
            traces: &self.traces,
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for State {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let repr = StateRepr::deserialize(deserializer)?;
        Ok(State::new(
            repr.config,
            repr.weights,
            repr.betas,
            repr.traces,
        ))
    }
}

/// Serializes the same fields as [`Idbd::snapshot`], without allocating.
impl Serialize for Idbd {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        StateRef {
            config: self.config(),
            weights: self.weights(),
            betas: self.log_step_sizes(),
            traces: self.traces(),
        }
        .serialize(serializer)
    }
}

/// Deserializes a [`State`] and restores it with [`Idbd::from_state`], so every check of
/// that function applies.
impl<'de> Deserialize<'de> for Idbd {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Idbd::from_state(State::deserialize(deserializer)?).map_err(D::Error::custom)
    }
}
