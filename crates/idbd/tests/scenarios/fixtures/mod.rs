//! Reference fixtures computed independently by `reference.py`.

mod reference_data;

pub(crate) use reference_data::{
    AUTOSTEP_DEFAULTS, AUTOSTEP_FAST, C06_C07, SEQUENCE_BOUNDED, SEQUENCE_UNBOUNDED,
};

use idbd::{Bounds, Config, autostep};

pub(crate) struct Fixture {
    pub(crate) initial_step_size: f64,
    pub(crate) meta_step_size: f64,
    pub(crate) min_beta: Option<f64>,
    pub(crate) max_beta: Option<f64>,
    pub(crate) max_beta_increment: Option<f64>,
    pub(crate) steps: &'static [Step],
}

pub(crate) struct Step {
    pub(crate) features: &'static [f64],
    pub(crate) target: f64,
    pub(crate) prediction: f64,
    pub(crate) error: f64,
    pub(crate) bounded_features: usize,
    pub(crate) weights: &'static [f64],
    pub(crate) betas: &'static [f64],
    pub(crate) traces: &'static [f64],
}

impl Fixture {
    pub(crate) fn config(&self) -> Config {
        let mut bounds = Bounds::none();
        if let Some(min) = self.min_beta {
            bounds = bounds.with_min_beta(min).expect("valid fixture floor");
        }
        if let Some(max) = self.max_beta {
            bounds = bounds.with_max_beta(max).expect("valid fixture ceiling");
        }
        if let Some(limit) = self.max_beta_increment {
            bounds = bounds
                .with_max_beta_increment(limit)
                .expect("valid fixture increment");
        }
        Config::new(self.initial_step_size, self.meta_step_size)
            .and_then(|c| c.with_bounds(bounds))
            .expect("valid fixture configuration")
    }

    pub(crate) fn num_features(&self) -> usize {
        self.steps[0].features.len()
    }
}

pub(crate) struct AutostepFixture {
    pub(crate) initial_step_size: f64,
    pub(crate) meta_step_size: f64,
    pub(crate) time_scale: f64,
    pub(crate) steps: &'static [AutostepStep],
}

pub(crate) struct AutostepStep {
    pub(crate) features: &'static [f64],
    pub(crate) target: f64,
    pub(crate) prediction: f64,
    pub(crate) error: f64,
    pub(crate) effective_step_size: f64,
    pub(crate) weights: &'static [f64],
    pub(crate) step_sizes: &'static [f64],
    pub(crate) traces: &'static [f64],
    pub(crate) normalizers: &'static [f64],
}

impl AutostepFixture {
    pub(crate) fn config(&self) -> autostep::Config {
        autostep::Config::new(self.initial_step_size)
            .and_then(|c| c.with_meta_step_size(self.meta_step_size))
            .and_then(|c| c.with_time_scale(self.time_scale))
            .expect("valid fixture configuration")
    }

    pub(crate) fn num_features(&self) -> usize {
        self.steps[0].features.len()
    }
}
