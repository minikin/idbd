//! WebAssembly adapter for the IDBD Explorer demo.
//!
//! The browser never implements IDBD. Every learning value it displays is produced by
//! the `idbd` core, driven either directly ([`Learner`]), through the lab's guided
//! example ([`Walkthrough`]) or through the lab's paired IDBD and LMS session
//! ([`Explorer`]). This crate only validates arguments, calls into those crates and
//! copies committed state across the boundary.

use idbd::autostep;
use idbd::{Config, Idbd, Update};
use idbd_lab::rng::GENERATOR;
use idbd_lab::session::{
    EVENT_CAPACITY, Failure, FailureCause, HISTORY_CAPACITY, Learner as FailedLearner,
    ROLLING_WINDOW, Session, Settings,
};
use idbd_lab::trace::CoordinateChange;
use idbd_lab::walkthrough::{
    WALKTHROUGH_COEFFICIENTS, WALKTHROUGH_FEATURES, WALKTHROUGH_INITIAL_STEP_SIZE,
    WALKTHROUGH_META_STEP_SIZE, Walkthrough as LabWalkthrough,
};
use idbd_lab::world::{Event, GROUP_SIZE, NUM_FEATURES, Origin, Preset};
use wasm_bindgen::JsError;
use wasm_bindgen::prelude::wasm_bindgen;

mod family;
pub use family::{FamilyDay, FamilyWatch, family_json, family_overlaps, family_score};

/// Largest whole number a JavaScript double represents exactly.
const MAX_SAFE_INTEGER: f64 = 9_007_199_254_740_991.0;

#[expect(
    clippy::cast_precision_loss,
    clippy::as_conversions,
    reason = "sample counts stay far below 2^53"
)]
fn count_to_f64(count: u64) -> f64 {
    count as f64
}

#[expect(
    clippy::cast_sign_loss,
    clippy::cast_possible_truncation,
    clippy::as_conversions,
    reason = "the condition checks for a whole number in 0..=2^53-1 before the cast"
)]
fn whole_number(value: f64) -> Option<u64> {
    (value.is_finite() && (0.0..=MAX_SAFE_INTEGER).contains(&value) && value.fract() == 0.0)
        .then_some(value as u64)
}

fn seed_from_number(seed: f64) -> Result<u64, String> {
    whole_number(seed)
        .ok_or_else(|| "seed must be a whole number between 0 and 2^53 - 1".to_owned())
}

fn sample_from_number(sample: f64) -> Result<u64, String> {
    whole_number(sample)
        .ok_or_else(|| "a sample index must be a whole number between 0 and 2^53 - 1".to_owned())
}

/// Carries a Rust message across the boundary as a JavaScript `Error`.
fn js(message: &str) -> JsError {
    JsError::new(message)
}

const FEATURE_OUT_OF_RANGE: &str = "feature index is out of range";

/// Writes a finite number, or `null`, so the result always parses as JSON.
fn json_number(value: f64) -> String {
    if value.is_finite() {
        format!("{value:?}")
    } else {
        "null".to_owned()
    }
}

/// Indices of up to `max_points` evenly spaced items of a `len`-item series.
///
/// Both ends are always included, so the sample axis and every value series picked with
/// the same `len` and `max_points` describe the same samples.
fn sample_indices(len: usize, max_points: usize) -> Vec<usize> {
    if len == 0 {
        return Vec::new();
    }
    let wanted = max_points.max(1);
    if len <= wanted || wanted == 1 {
        return (0..len).collect();
    }
    let span = len - 1;
    let steps = wanted - 1;
    (0..wanted)
        .map(|i| (i * span + steps / 2) / steps)
        .collect()
}

fn coordinate_json(change: &CoordinateChange) -> String {
    let fields = [
        ("input", change.input),
        ("oldWeight", change.old_weight),
        ("newWeight", change.new_weight),
        ("weightChange", change.weight_change()),
        ("oldBeta", change.old_beta),
        ("newBeta", change.new_beta),
        ("betaChange", change.beta_change()),
        ("oldAlpha", change.old_alpha()),
        ("newAlpha", change.new_alpha()),
        ("oldTrace", change.old_trace),
        ("newTrace", change.new_trace),
    ];
    let body = fields
        .iter()
        .map(|(name, value)| format!("\"{name}\":{}", json_number(*value)))
        .collect::<Vec<_>>()
        .join(",");
    format!("{{{body}}}")
}

fn event_json(sample: u64, event: Event) -> String {
    let sample = json_number(count_to_f64(sample));
    match event {
        Event::SignFlip { feature } => {
            format!("{{\"sample\":{sample},\"kind\":\"signFlip\",\"feature\":{feature}}}")
        }
        Event::RelevanceShift { from, to, origin } => {
            let origin = match origin {
                Origin::Scheduled => "scheduled",
                Origin::Requested => "requested",
            };
            format!(
                "{{\"sample\":{sample},\"kind\":\"relevanceShift\",\"from\":{from},\"to\":{to},\"origin\":\"{origin}\"}}"
            )
        }
    }
}

fn failed_learner_name(failure: &Failure) -> &'static str {
    match failure.learner() {
        FailedLearner::Idbd => "IDBD",
        FailedLearner::Lms => "LMS",
        FailedLearner::Autostep => "Autostep",
    }
}

fn failure_cause_message(failure: &Failure) -> String {
    match failure.cause {
        FailureCause::Idbd(e) | FailureCause::Autostep(e) => e.to_string(),
        FailureCause::Lms(e) => e.to_string(),
    }
}

/// Version of the Rust core and lab this module was built from.
#[must_use]
#[wasm_bindgen(js_name = coreVersion)]
pub fn core_version() -> String {
    env!("CARGO_PKG_VERSION").to_owned()
}

/// Identifier of the random generator that draws every input and event.
#[must_use]
#[wasm_bindgen(js_name = generatorId)]
pub fn generator_id() -> String {
    GENERATOR.to_owned()
}

/// Number of inputs in every experiment task.
#[must_use]
#[wasm_bindgen(js_name = numFeatures)]
pub fn num_features() -> usize {
    NUM_FEATURES
}

/// Number of inputs that are relevant at any one time.
#[must_use]
#[wasm_bindgen(js_name = groupSize)]
pub fn group_size() -> usize {
    GROUP_SIZE
}

/// Number of recent pre-update errors in each rolling mean squared error.
#[must_use]
#[wasm_bindgen(js_name = rollingWindow)]
pub fn rolling_window() -> usize {
    ROLLING_WINDOW
}

/// Number of recent samples kept for charts.
#[must_use]
#[wasm_bindgen(js_name = historyCapacity)]
pub fn history_capacity() -> usize {
    HISTORY_CAPACITY
}

/// Number of recent world events kept for the timeline.
#[must_use]
#[wasm_bindgen(js_name = eventCapacity)]
pub fn event_capacity() -> usize {
    EVENT_CAPACITY
}

/// Returns the selectable presets as JSON: id and whether they come from the paper.
#[must_use]
#[wasm_bindgen(js_name = presetsJson)]
pub fn presets_json() -> String {
    let body = Preset::ALL
        .iter()
        .map(|preset| {
            format!(
                "{{\"id\":\"{}\",\"fromPaper\":{}}}",
                preset.id(),
                preset.is_from_paper()
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    format!("[{body}]")
}

/// A three-input IDBD learner that explains one update at a time.
///
/// Wraps [`idbd_lab::walkthrough::Walkthrough`], which feeds the core learner a seeded
/// sample and records each coordinate's committed values before and after the update.
#[wasm_bindgen]
#[derive(Debug)]
pub struct Walkthrough {
    inner: LabWalkthrough,
}

/// The adapter's own fallible operations. The exported methods below wrap these and
/// turn the message into a JavaScript `Error`.
impl Walkthrough {
    fn create(seed: f64) -> Result<Self, String> {
        Ok(Self {
            inner: LabWalkthrough::new(seed_from_number(seed)?),
        })
    }

    fn advance_sample(&mut self) -> Result<(), String> {
        self.inner.advance().map(|_| ()).map_err(|e| e.to_string())
    }

    fn coordinate(&self, feature: usize) -> Result<String, String> {
        self.inner
            .changes()
            .get(feature)
            .map(coordinate_json)
            .ok_or_else(|| FEATURE_OUT_OF_RANGE.to_owned())
    }
}

#[wasm_bindgen]
impl Walkthrough {
    /// Starts a walkthrough at sample 0.
    ///
    /// # Errors
    ///
    /// Returns an error unless `seed` is a whole number in `0..2^53`.
    #[wasm_bindgen(constructor)]
    pub fn new(seed: f64) -> Result<Self, JsError> {
        Self::create(seed).map_err(|message| js(&message))
    }

    /// Draws the next sample and lets the core learner learn from it.
    ///
    /// # Errors
    ///
    /// Returns the core error message if the core rejects the sample.
    pub fn advance(&mut self) -> Result<(), JsError> {
        self.advance_sample().map_err(|message| js(&message))
    }

    /// Returns the number of completed samples.
    #[must_use]
    pub fn sample(&self) -> f64 {
        count_to_f64(self.inner.sample())
    }

    /// Returns the number of inputs.
    #[must_use]
    #[wasm_bindgen(js_name = numFeatures)]
    pub fn num_features(&self) -> usize {
        WALKTHROUGH_FEATURES
    }

    /// Returns the inputs of the last sample.
    #[must_use]
    pub fn features(&self) -> Vec<f64> {
        self.inner.features().to_vec()
    }

    /// Returns the true coefficients of the walkthrough task.
    #[must_use]
    pub fn coefficients(&self) -> Vec<f64> {
        WALKTHROUGH_COEFFICIENTS.to_vec()
    }

    /// Returns the target of the last sample.
    #[must_use]
    pub fn target(&self) -> f64 {
        self.inner.target()
    }

    /// Returns the prediction made before learning from the last sample.
    #[must_use]
    pub fn prediction(&self) -> Option<f64> {
        self.inner.report().map(|r| r.prediction)
    }

    /// Returns `target - prediction` for the last sample, measured before learning.
    #[must_use]
    pub fn error(&self) -> Option<f64> {
        self.inner.report().map(|r| r.error)
    }

    /// Returns the step size each input started with.
    #[must_use]
    #[wasm_bindgen(js_name = initialStepSize)]
    pub fn initial_step_size(&self) -> f64 {
        WALKTHROUGH_INITIAL_STEP_SIZE
    }

    /// Returns the meta step size theta.
    #[must_use]
    pub fn theta(&self) -> f64 {
        WALKTHROUGH_META_STEP_SIZE
    }

    /// Returns the committed weights.
    #[must_use]
    pub fn weights(&self) -> Vec<f64> {
        self.inner.learner().weights().to_vec()
    }

    /// Returns the committed step sizes.
    #[must_use]
    #[wasm_bindgen(js_name = stepSizes)]
    pub fn step_sizes(&self) -> Vec<f64> {
        self.inner.learner().step_sizes().collect()
    }

    /// Returns the committed traces.
    #[must_use]
    pub fn traces(&self) -> Vec<f64> {
        self.inner.learner().traces().to_vec()
    }

    /// Returns one coordinate's values before and after the last update, as JSON.
    ///
    /// # Errors
    ///
    /// Returns an error if `feature` is out of range.
    #[wasm_bindgen(js_name = coordinateJson)]
    pub fn coordinate_json(&self, feature: usize) -> Result<String, JsError> {
        self.coordinate(feature).map_err(|message| js(&message))
    }
}

/// A paired IDBD and LMS experiment on one shared sample stream.
///
/// Wraps [`idbd_lab::session::Session`]. Every accessor describes the last sample both
/// learners completed.
#[wasm_bindgen]
#[derive(Debug)]
pub struct Explorer {
    session: Session,
}

/// Autostep's starting step size in the demo: 0.1 / n, the rule the paper used on its
/// idealized problems. Mu and tau are the paper's defaults, so the demo shows Autostep
/// untuned.
const AUTOSTEP_INITIAL_STEP_SIZE: f64 = 0.1 / 20.0;

const _: () = assert!(
    NUM_FEATURES == 20,
    "AUTOSTEP_INITIAL_STEP_SIZE assumes 20 inputs"
);

/// The adapter's own fallible operations. The exported methods below wrap these and
/// turn the message into a JavaScript `Error`.
impl Explorer {
    fn create(
        preset_id: &str,
        seed: f64,
        initial_step_size: f64,
        theta: f64,
        lms_step_size: f64,
    ) -> Result<Self, String> {
        let preset =
            Preset::from_id(preset_id).ok_or_else(|| format!("unknown preset id: {preset_id}"))?;
        let idbd = Config::new(initial_step_size, theta).map_err(|e| e.to_string())?;
        let settings = Settings {
            preset,
            seed: seed_from_number(seed)?,
            idbd,
            lms_step_size,
            autostep: autostep::Config::new(AUTOSTEP_INITIAL_STEP_SIZE)
                .expect("the constant is a valid step size"),
        };
        Session::new(settings)
            .map(|session| Self { session })
            .map_err(|e| e.to_string())
    }

    fn advance_one(&mut self) -> Result<(), String> {
        self.session.step().map_err(|failure| failure.to_string())
    }

    fn advance(&mut self, samples: u32) -> Result<u32, String> {
        match self.session.run(u64::from(samples)) {
            Ok(done) => Ok(u32::try_from(done).unwrap_or(samples)),
            Err(failure) => Err(failure.to_string()),
        }
    }

    fn schedule(&mut self, sample: f64) -> Result<(), String> {
        let sample = sample_from_number(sample)?;
        self.session
            .schedule_change(sample)
            .map_err(|e| e.to_string())
    }

    fn coordinate(&self, feature: usize) -> Result<String, String> {
        self.session
            .changes()
            .get(feature)
            .map(coordinate_json)
            .ok_or_else(|| FEATURE_OUT_OF_RANGE.to_owned())
    }

    fn alpha_series(&self, feature: usize, max_points: usize) -> Result<Vec<f64>, String> {
        if feature >= NUM_FEATURES {
            return Err(FEATURE_OUT_OF_RANGE.to_owned());
        }
        let alphas: Vec<&[f64; NUM_FEATURES]> = self.session.alpha_history().collect();
        Ok(sample_indices(alphas.len(), max_points)
            .into_iter()
            .map(|i| alphas[i][feature])
            .collect())
    }

    fn history_series(
        &self,
        max_points: usize,
        pick: impl Fn(&idbd_lab::session::Record) -> f64,
    ) -> Vec<f64> {
        let records: Vec<&idbd_lab::session::Record> = self.session.history().collect();
        sample_indices(records.len(), max_points)
            .into_iter()
            .map(|i| pick(records[i]))
            .collect()
    }
}

#[wasm_bindgen]
impl Explorer {
    /// Starts a run at sample 0.
    ///
    /// # Errors
    ///
    /// Returns an error for an unknown preset id, an invalid seed, an invalid IDBD
    /// configuration or a non-positive LMS step size.
    #[wasm_bindgen(constructor)]
    pub fn new(
        preset_id: &str,
        seed: f64,
        initial_step_size: f64,
        theta: f64,
        lms_step_size: f64,
    ) -> Result<Self, JsError> {
        Self::create(preset_id, seed, initial_step_size, theta, lms_step_size)
            .map_err(|message| js(&message))
    }

    /// Processes one shared sample.
    ///
    /// # Errors
    ///
    /// Returns the failure message if either learner rejects the sample, or if an
    /// earlier failure already stopped the run. Nothing learns from a failed sample.
    pub fn step(&mut self) -> Result<(), JsError> {
        self.advance_one().map_err(|message| js(&message))
    }

    /// Processes up to `samples` shared samples and returns the number completed.
    ///
    /// # Errors
    ///
    /// Returns the failure message when the first attempted sample fails. A failure
    /// after some progress returns the number completed instead; the failure is then
    /// readable through [`Explorer::has_failed`].
    pub fn run(&mut self, samples: u32) -> Result<u32, JsError> {
        self.advance(samples).map_err(|message| js(&message))
    }

    /// Asks the task to move relevance to the next group of inputs before the next
    /// sample. Neither learner changes until that sample is processed.
    #[wasm_bindgen(js_name = changeTheWorld)]
    pub fn change_the_world(&mut self) {
        self.session.change_the_world();
    }

    /// Schedules the change that "Change the world" makes to apply before `sample`.
    ///
    /// Replaying [`Explorer::requested_changes`] on a new explorer with the same
    /// settings repeats the run.
    ///
    /// # Errors
    ///
    /// Returns an error if `sample` is not a whole number, is already completed, or if
    /// too many changes already wait.
    #[wasm_bindgen(js_name = scheduleChange)]
    pub fn schedule_change(&mut self, sample: f64) -> Result<(), JsError> {
        self.schedule(sample).map_err(|message| js(&message))
    }

    /// Returns every sample that a requested change applied before, in order.
    #[must_use]
    #[wasm_bindgen(js_name = requestedChanges)]
    pub fn requested_changes(&self) -> Vec<f64> {
        self.session
            .requested_changes()
            .iter()
            .map(|&sample| count_to_f64(sample))
            .collect()
    }

    /// Returns the samples that scheduled changes still wait for, in order.
    #[must_use]
    #[wasm_bindgen(js_name = scheduledChanges)]
    pub fn scheduled_changes(&self) -> Vec<f64> {
        self.session.scheduled_changes().map(count_to_f64).collect()
    }

    /// Returns `true` while a requested change waits for the next sample.
    #[must_use]
    #[wasm_bindgen(js_name = changePending)]
    pub fn change_pending(&self) -> bool {
        self.session.change_pending()
    }

    /// Returns the last sample index completed by both learners; 0 before any sample.
    #[must_use]
    pub fn completed(&self) -> f64 {
        count_to_f64(self.session.completed())
    }

    /// Returns the preset id of this run.
    #[must_use]
    pub fn preset(&self) -> String {
        self.session.settings().preset.id().to_owned()
    }

    /// Returns the seed of this run.
    #[must_use]
    pub fn seed(&self) -> f64 {
        count_to_f64(self.session.settings().seed)
    }

    /// Returns `true` if this run's task comes from Sutton (1992).
    #[must_use]
    #[wasm_bindgen(js_name = presetFromPaper)]
    pub fn preset_from_paper(&self) -> bool {
        self.session.settings().preset.is_from_paper()
    }

    /// Returns the step size every IDBD input started with.
    #[must_use]
    #[wasm_bindgen(js_name = initialStepSize)]
    pub fn initial_step_size(&self) -> f64 {
        self.session.settings().idbd.initial_step_size()
    }

    /// Returns the IDBD meta step size theta.
    #[must_use]
    pub fn theta(&self) -> f64 {
        self.session.settings().idbd.meta_step_size()
    }

    /// Returns the fixed LMS step size.
    #[must_use]
    #[wasm_bindgen(js_name = lmsStepSize)]
    pub fn lms_step_size(&self) -> f64 {
        self.session.lms().step_size()
    }

    /// Returns the number of inputs.
    #[must_use]
    #[wasm_bindgen(js_name = numFeatures)]
    pub fn num_features(&self) -> usize {
        NUM_FEATURES
    }

    /// Returns the inputs of the last completed sample.
    #[must_use]
    pub fn features(&self) -> Vec<f64> {
        self.session.features().to_vec()
    }

    /// Returns the target of the last completed sample.
    #[must_use]
    pub fn target(&self) -> f64 {
        self.session.target()
    }

    /// Returns the true coefficients that generated the last completed sample.
    #[must_use]
    pub fn coefficients(&self) -> Vec<f64> {
        self.session.coefficients().to_vec()
    }

    /// Returns the IDBD weights after the last completed sample.
    #[must_use]
    #[wasm_bindgen(js_name = idbdWeights)]
    pub fn idbd_weights(&self) -> Vec<f64> {
        self.session.idbd().weights().to_vec()
    }

    /// Returns the IDBD step sizes after the last completed sample.
    #[must_use]
    #[wasm_bindgen(js_name = idbdStepSizes)]
    pub fn idbd_step_sizes(&self) -> Vec<f64> {
        self.session.idbd().step_sizes().collect()
    }

    /// Returns the IDBD traces after the last completed sample.
    #[must_use]
    #[wasm_bindgen(js_name = idbdTraces)]
    pub fn idbd_traces(&self) -> Vec<f64> {
        self.session.idbd().traces().to_vec()
    }

    /// Returns the LMS weights after the last completed sample.
    #[must_use]
    #[wasm_bindgen(js_name = lmsWeights)]
    pub fn lms_weights(&self) -> Vec<f64> {
        self.session.lms().weights().to_vec()
    }

    /// Returns the IDBD prediction made before learning the last completed sample.
    #[must_use]
    #[wasm_bindgen(js_name = idbdPrediction)]
    pub fn idbd_prediction(&self) -> Option<f64> {
        self.session.last_idbd_update().map(|r| r.prediction)
    }

    /// Returns the IDBD pre-update error of the last completed sample.
    #[must_use]
    #[wasm_bindgen(js_name = idbdError)]
    pub fn idbd_error(&self) -> Option<f64> {
        self.session.last_idbd_update().map(|r| r.error)
    }

    /// Returns the LMS pre-update error of the last completed sample.
    #[must_use]
    #[wasm_bindgen(js_name = lmsError)]
    pub fn lms_error(&self) -> Option<f64> {
        self.session.last_lms_error()
    }

    /// Returns the IDBD rolling mean squared error over the last
    /// `min(100, completed)` pre-update errors.
    #[must_use]
    #[wasm_bindgen(js_name = idbdRollingMse)]
    pub fn idbd_rolling_mse(&self) -> Option<f64> {
        self.session.idbd_rolling_mse()
    }

    /// Returns the LMS rolling mean squared error over the same window.
    #[must_use]
    #[wasm_bindgen(js_name = lmsRollingMse)]
    pub fn lms_rolling_mse(&self) -> Option<f64> {
        self.session.lms_rolling_mse()
    }

    /// Returns the IDBD mean squared error over every completed sample.
    #[must_use]
    #[wasm_bindgen(js_name = idbdMeanSquaredError)]
    pub fn idbd_mean_squared_error(&self) -> f64 {
        let totals = self.session.totals();
        if totals.samples == 0 {
            return 0.0;
        }
        totals.idbd_squared_error / count_to_f64(totals.samples)
    }

    /// Returns the LMS mean squared error over every completed sample.
    #[must_use]
    #[wasm_bindgen(js_name = lmsMeanSquaredError)]
    pub fn lms_mean_squared_error(&self) -> f64 {
        let totals = self.session.totals();
        if totals.samples == 0 {
            return 0.0;
        }
        totals.lms_squared_error / count_to_f64(totals.samples)
    }

    /// Returns the Autostep mean squared error over every completed sample.
    #[must_use]
    #[wasm_bindgen(js_name = autostepMeanSquaredError)]
    pub fn autostep_mean_squared_error(&self) -> f64 {
        let totals = self.session.totals();
        if totals.samples == 0 {
            return 0.0;
        }
        totals.autostep_squared_error / count_to_f64(totals.samples)
    }

    /// Returns the Autostep weights after the last completed sample.
    #[must_use]
    #[wasm_bindgen(js_name = autostepWeights)]
    pub fn autostep_weights(&self) -> Vec<f64> {
        self.session.autostep().weights().to_vec()
    }

    /// Returns the Autostep step sizes after the last completed sample.
    #[must_use]
    #[wasm_bindgen(js_name = autostepStepSizes)]
    pub fn autostep_step_sizes(&self) -> Vec<f64> {
        self.session.autostep().step_sizes().to_vec()
    }

    /// Returns the Autostep pre-update error of the last completed sample.
    #[must_use]
    #[wasm_bindgen(js_name = autostepError)]
    pub fn autostep_error(&self) -> Option<f64> {
        self.session.last_autostep_error()
    }

    /// Returns the Autostep rolling mean squared error over the last
    /// `min(100, completed)` pre-update errors.
    #[must_use]
    #[wasm_bindgen(js_name = autostepRollingMse)]
    pub fn autostep_rolling_mse(&self) -> Option<f64> {
        self.session.autostep_rolling_mse()
    }

    /// Returns Autostep's configuration as JSON: initial step size, mu and tau.
    #[must_use]
    #[wasm_bindgen(js_name = autostepConfigJson)]
    pub fn autostep_config_json(&self) -> String {
        let config = self.session.settings().autostep;
        format!(
            "{{\"initialStepSize\":{},\"metaStepSize\":{},\"timeScale\":{}}}",
            json_number(config.initial_step_size()),
            json_number(config.meta_step_size()),
            json_number(config.time_scale())
        )
    }

    /// Returns one coordinate's values before and after the last completed update, as
    /// JSON.
    ///
    /// # Errors
    ///
    /// Returns an error if `feature` is out of range.
    #[wasm_bindgen(js_name = coordinateJson)]
    pub fn coordinate_json(&self, feature: usize) -> Result<String, JsError> {
        self.coordinate(feature).map_err(|message| js(&message))
    }

    /// Returns the number of samples retained for charts.
    #[must_use]
    #[wasm_bindgen(js_name = historyLength)]
    pub fn history_length(&self) -> usize {
        self.session.history().len()
    }

    /// Returns up to `max_points` evenly spaced retained sample indices, oldest first.
    #[must_use]
    #[wasm_bindgen(js_name = historySamples)]
    pub fn history_samples(&self, max_points: usize) -> Vec<f64> {
        self.history_series(max_points, |record| count_to_f64(record.sample))
    }

    /// Returns the IDBD pre-update errors for those samples.
    #[must_use]
    #[wasm_bindgen(js_name = historyIdbdError)]
    pub fn history_idbd_error(&self, max_points: usize) -> Vec<f64> {
        self.history_series(max_points, |record| record.idbd_error)
    }

    /// Returns the LMS pre-update errors for those samples.
    #[must_use]
    #[wasm_bindgen(js_name = historyLmsError)]
    pub fn history_lms_error(&self, max_points: usize) -> Vec<f64> {
        self.history_series(max_points, |record| record.lms_error)
    }

    /// Returns the IDBD rolling mean squared error for those samples.
    #[must_use]
    #[wasm_bindgen(js_name = historyIdbdRollingMse)]
    pub fn history_idbd_rolling_mse(&self, max_points: usize) -> Vec<f64> {
        self.history_series(max_points, |record| record.idbd_rolling_mse)
    }

    /// Returns the LMS rolling mean squared error for those samples.
    #[must_use]
    #[wasm_bindgen(js_name = historyLmsRollingMse)]
    pub fn history_lms_rolling_mse(&self, max_points: usize) -> Vec<f64> {
        self.history_series(max_points, |record| record.lms_rolling_mse)
    }

    /// Returns up to `max_points` evenly spaced Autostep rolling MSE values, oldest
    /// first, for the same samples as [`Explorer::history_samples`].
    #[must_use]
    #[wasm_bindgen(js_name = historyAutostepRollingMse)]
    pub fn history_autostep_rolling_mse(&self, max_points: usize) -> Vec<f64> {
        self.history_series(max_points, |record| record.autostep_rolling_mse)
    }

    /// Returns up to `max_points` evenly spaced step sizes of one input, oldest first,
    /// for the same samples as [`Explorer::history_samples`].
    ///
    /// # Errors
    ///
    /// Returns an error if `feature` is out of range.
    #[wasm_bindgen(js_name = alphaHistory)]
    pub fn alpha_history(&self, feature: usize, max_points: usize) -> Result<Vec<f64>, JsError> {
        self.alpha_series(feature, max_points)
            .map_err(|message| js(&message))
    }

    /// Returns the retained world events as JSON, oldest first.
    #[must_use]
    #[wasm_bindgen(js_name = eventsJson)]
    pub fn events_json(&self) -> String {
        let body = self
            .session
            .events()
            .map(|timed| event_json(timed.sample, timed.event))
            .collect::<Vec<_>>()
            .join(",");
        format!("[{body}]")
    }

    /// Returns `true` once a sample has failed. The run cannot continue after that.
    #[must_use]
    #[wasm_bindgen(js_name = hasFailed)]
    pub fn has_failed(&self) -> bool {
        self.session.failure().is_some()
    }

    /// Returns the index of the attempted sample that failed.
    #[must_use]
    #[wasm_bindgen(js_name = failureSample)]
    pub fn failure_sample(&self) -> Option<f64> {
        self.session
            .failure()
            .map(|failure| count_to_f64(failure.sample))
    }

    /// Returns the name of the learner that rejected the sample.
    #[must_use]
    #[wasm_bindgen(js_name = failureLearner)]
    pub fn failure_learner(&self) -> Option<String> {
        self.session
            .failure()
            .map(|failure| failed_learner_name(failure).to_owned())
    }

    /// Returns the numerical reason the sample was rejected.
    #[must_use]
    #[wasm_bindgen(js_name = failureCause)]
    pub fn failure_cause(&self) -> Option<String> {
        self.session.failure().map(failure_cause_message)
    }

    /// Returns the full failure message, learner and sample index included.
    #[must_use]
    #[wasm_bindgen(js_name = failureMessage)]
    pub fn failure_message(&self) -> Option<String> {
        self.session.failure().map(Failure::to_string)
    }
}

/// The core IDBD learner, driven directly with caller-supplied samples.
///
/// The demo uses it to replay the crate's reference fixtures in the browser.
#[wasm_bindgen]
#[derive(Debug)]
pub struct Learner {
    learner: Idbd,
    last: Option<Update>,
}

/// Largest feature count a browser caller may request. JavaScript converts `-1` to
/// `usize::MAX`, and a failed allocation aborts the whole module instead of throwing.
const MAX_LEARNER_FEATURES: usize = 10_000;

/// The adapter's own fallible operations. The exported methods below wrap these and
/// turn the message into a JavaScript `Error`.
impl Learner {
    fn create(num_features: usize, initial_step_size: f64, theta: f64) -> Result<Self, String> {
        if num_features > MAX_LEARNER_FEATURES {
            return Err(format!(
                "a browser learner supports at most {MAX_LEARNER_FEATURES} features, got {num_features}"
            ));
        }
        let config = Config::new(initial_step_size, theta).map_err(|e| e.to_string())?;
        Idbd::new(num_features, config)
            .map(|learner| Self {
                learner,
                last: None,
            })
            .map_err(|e| e.to_string())
    }

    fn predict_only(&self, features: &[f64]) -> Result<f64, String> {
        self.learner.predict(features).map_err(|e| e.to_string())
    }

    fn learn(&mut self, features: &[f64], target: f64) -> Result<(), String> {
        let report = self
            .learner
            .update(features, target)
            .map_err(|e| e.to_string())?;
        self.last = Some(report);
        Ok(())
    }
}

#[wasm_bindgen]
impl Learner {
    /// Creates a learner with zero weights and zero traces.
    ///
    /// # Errors
    ///
    /// Returns an error for zero features or an invalid step size or theta.
    #[wasm_bindgen(constructor)]
    pub fn new(num_features: usize, initial_step_size: f64, theta: f64) -> Result<Self, JsError> {
        Self::create(num_features, initial_step_size, theta).map_err(|message| js(&message))
    }

    /// Predicts without learning.
    ///
    /// # Errors
    ///
    /// Returns the core error message for a wrong-length or non-finite input vector.
    pub fn predict(&self, features: &[f64]) -> Result<f64, JsError> {
        self.predict_only(features).map_err(|message| js(&message))
    }

    /// Learns from one example. The committed state is unchanged on error.
    ///
    /// # Errors
    ///
    /// Returns the core error message.
    pub fn update(&mut self, features: &[f64], target: f64) -> Result<(), JsError> {
        self.learn(features, target).map_err(|message| js(&message))
    }

    /// Returns the number of inputs.
    #[must_use]
    #[wasm_bindgen(js_name = numFeatures)]
    pub fn num_features(&self) -> usize {
        self.learner.num_features()
    }

    /// Returns the prediction made before learning from the last example.
    #[must_use]
    pub fn prediction(&self) -> Option<f64> {
        self.last.map(|r| r.prediction)
    }

    /// Returns `target - prediction` for the last example, measured before learning.
    #[must_use]
    pub fn error(&self) -> Option<f64> {
        self.last.map(|r| r.error)
    }

    /// Returns how many coordinates had their beta clipped by a bound.
    #[must_use]
    #[wasm_bindgen(js_name = boundedFeatures)]
    pub fn bounded_features(&self) -> Option<u32> {
        self.last
            .and_then(|r| u32::try_from(r.bounded_features).ok())
    }

    /// Returns the committed weights.
    #[must_use]
    pub fn weights(&self) -> Vec<f64> {
        self.learner.weights().to_vec()
    }

    /// Returns the committed log step sizes, beta.
    #[must_use]
    #[wasm_bindgen(js_name = logStepSizes)]
    pub fn log_step_sizes(&self) -> Vec<f64> {
        self.learner.log_step_sizes().to_vec()
    }

    /// Returns the committed step sizes, alpha.
    #[must_use]
    #[wasm_bindgen(js_name = stepSizes)]
    pub fn step_sizes(&self) -> Vec<f64> {
        self.learner.step_sizes().collect()
    }

    /// Returns the committed traces.
    #[must_use]
    pub fn traces(&self) -> Vec<f64> {
        self.learner.traces().to_vec()
    }
}

#[cfg(test)]
#[expect(clippy::float_cmp, reason = "tests assert exact, deterministic values")]
mod tests {
    use super::*;

    #[test]
    fn sample_indices_keep_both_ends_and_never_exceed_the_budget() {
        assert_eq!(sample_indices(0, 10), Vec::<usize>::new());
        assert_eq!(sample_indices(3, 10), vec![0, 1, 2]);
        let picked = sample_indices(2_000, 240);
        assert_eq!(picked.len(), 240);
        assert_eq!(picked.first().copied(), Some(0));
        assert_eq!(picked.last().copied(), Some(1_999));
        assert!(
            picked.windows(2).all(|w| w[0] < w[1]),
            "strictly increasing"
        );
    }

    #[test]
    fn a_seed_must_be_a_whole_non_negative_number() {
        assert!(seed_from_number(0.0).is_ok());
        assert!(seed_from_number(7.0).is_ok());
        for bad in [-1.0, 0.5, f64::NAN, f64::INFINITY, 1e300] {
            assert!(seed_from_number(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn d06_a_schedule_replays_through_the_adapter() {
        let mut original =
            Explorer::create("sutton1992", 3.0, 0.05, 0.01, 0.03).expect("valid settings");
        original.advance(20).expect("run");
        original.change_the_world();
        original.advance(30).expect("run");
        assert_eq!(original.requested_changes(), [21.0]);

        let mut replay =
            Explorer::create("sutton1992", 3.0, 0.05, 0.01, 0.03).expect("valid settings");
        replay.schedule(21.0).expect("future sample");
        assert_eq!(replay.scheduled_changes(), [21.0]);
        replay.advance(50).expect("run");
        assert_eq!(replay.idbd_weights(), original.idbd_weights());
        assert_eq!(replay.requested_changes(), original.requested_changes());
        assert!(replay.scheduled_changes().is_empty());

        let past = replay.schedule(50.0).expect_err("completed sample");
        assert!(past.contains("already completed"), "{past}");
        let fraction = replay.schedule(60.5).expect_err("not a whole number");
        assert!(fraction.starts_with("a sample index must be"), "{fraction}");
    }

    #[test]
    fn non_finite_numbers_serialize_as_null() {
        assert_eq!(json_number(0.5), "0.5");
        assert_eq!(json_number(f64::NAN), "null");
        assert_eq!(json_number(f64::INFINITY), "null");
    }

    #[test]
    fn oversized_learners_are_rejected_before_allocating() {
        let message = Learner::create(usize::MAX, 0.1, 0.01).expect_err("too many features");
        assert!(message.contains("at most 10000 features"), "{message}");
        assert!(
            Learner::create(MAX_LEARNER_FEATURES, 0.1, 0.01).is_ok(),
            "limit is inclusive"
        );
        assert!(Learner::create(0, 0.1, 0.01).is_err(), "zero features");
    }

    #[test]
    fn d01_c06_and_c07_fixtures_go_through_the_core() {
        let mut learner = Learner::create(2, 0.1, 0.01).expect("standard learner");
        learner.learn(&[1.0, 2.0], 1.0).expect("C06");
        learner.learn(&[2.0, -1.0], 0.5).expect("C07");
        assert_eq!(learner.prediction(), Some(0.0));
        assert_eq!(learner.error(), Some(0.5));
        let weights = learner.weights();
        let traces = learner.traces();
        let betas = learner.log_step_sizes();
        let expected_weights = [0.200_100_050_016_670_86, 0.150_049_975_008_331_25];
        let expected_traces = [0.160_060_030_010_002_52, 0.130_069_965_011_663_76];
        let expected_betas = [0.1_f64.ln() + 0.001, 0.1_f64.ln() - 0.001];
        for i in 0..2 {
            assert!(
                close(weights[i], expected_weights[i]),
                "weight {i}: {} vs {}",
                weights[i],
                expected_weights[i]
            );
            assert!(
                close(traces[i], expected_traces[i]),
                "trace {i}: {} vs {}",
                traces[i],
                expected_traces[i]
            );
            assert!(
                close(betas[i], expected_betas[i]),
                "beta {i}: {} vs {}",
                betas[i],
                expected_betas[i]
            );
        }
    }

    #[test]
    fn an_explorer_advances_both_learners_on_one_shared_stream() {
        let mut explorer =
            Explorer::create("sutton1992", 1.0, 0.05, 0.01, 0.04).expect("valid settings");
        assert_eq!(explorer.completed(), 0.0);
        explorer.advance(120).expect("run");
        assert_eq!(explorer.completed(), 120.0);
        assert!(!explorer.has_failed());
        assert_eq!(explorer.idbd_weights().len(), NUM_FEATURES);
        assert_eq!(explorer.lms_weights().len(), NUM_FEATURES);
        assert_eq!(explorer.history_samples(60).len(), 60);
        assert_eq!(explorer.alpha_series(0, 60).expect("feature 0").len(), 60);
        assert!(explorer.alpha_series(NUM_FEATURES, 10).is_err());
        assert!(explorer.events_json().contains("signFlip"), "sign flips");
        let coordinate = explorer.coordinate(0).expect("feature 0");
        assert!(coordinate.contains("\"newAlpha\""), "{coordinate}");
    }

    #[test]
    fn an_explorer_runs_autostep_untuned_on_the_same_stream() {
        let mut explorer =
            Explorer::create("sutton1992", 1.0, 0.05, 0.01, 0.04).expect("valid settings");
        assert_eq!(explorer.autostep_mean_squared_error(), 0.0);
        assert_eq!(explorer.autostep_error(), None);
        assert_eq!(explorer.autostep_rolling_mse(), None);
        assert_eq!(
            explorer.autostep_config_json(),
            r#"{"initialStepSize":0.005,"metaStepSize":0.01,"timeScale":10000.0}"#
        );
        explorer.advance(120).expect("run");
        assert_eq!(explorer.autostep_weights().len(), NUM_FEATURES);
        assert_eq!(explorer.autostep_step_sizes().len(), NUM_FEATURES);
        let error = explorer.autostep_error().expect("a sample completed");
        let last = explorer.session.history().last().copied().expect("history");
        assert_eq!(error, last.autostep_error);
        assert_eq!(
            explorer.autostep_rolling_mse(),
            Some(last.autostep_rolling_mse)
        );
        let rolling = explorer.history_autostep_rolling_mse(60);
        assert_eq!(rolling.len(), 60);
        assert_eq!(rolling.last().copied(), Some(last.autostep_rolling_mse));
        let total = explorer.session.totals().autostep_squared_error;
        assert_eq!(explorer.autostep_mean_squared_error(), total / 120.0);
    }

    #[test]
    fn an_autostep_failure_is_named_and_explained() {
        let error = idbd::Error::Numerical {
            stage: idbd::Stage::StepSize,
            coordinate: Some(3),
        };
        let failure = Failure {
            sample: 9,
            cause: FailureCause::Autostep(error),
        };
        assert_eq!(failed_learner_name(&failure), "Autostep");
        assert_eq!(failure_cause_message(&failure), error.to_string());
    }

    #[test]
    fn change_the_world_waits_for_the_next_sample() {
        let mut explorer =
            Explorer::create("stationary", 2.0, 0.05, 0.01, 0.04).expect("valid settings");
        explorer.advance(50).expect("run");
        let before = explorer.idbd_weights();
        explorer.change_the_world();
        assert!(explorer.change_pending());
        assert_eq!(explorer.idbd_weights(), before);
        explorer.advance_one().expect("step");
        assert!(!explorer.change_pending());
        assert!(
            explorer.events_json().contains("relevanceShift"),
            "{}",
            explorer.events_json()
        );
    }

    #[test]
    fn a_failure_is_reported_and_refuses_further_steps() {
        let mut explorer =
            Explorer::create("sutton1992", 8.0, 0.05, 0.01, 5.0).expect("valid settings");
        while explorer.advance_one().is_ok() {
            assert!(explorer.completed() < 10_000.0, "LMS should diverge");
        }
        assert!(explorer.has_failed());
        assert_eq!(explorer.failure_learner().as_deref(), Some("LMS"));
        assert!(explorer.failure_sample().is_some());
        assert!(explorer.failure_message().is_some());
        assert!(
            explorer.advance_one().is_err(),
            "a frozen run refuses steps"
        );
    }

    #[test]
    fn invalid_arguments_are_rejected() {
        assert!(Explorer::create("nope", 1.0, 0.05, 0.01, 0.04).is_err());
        assert!(Explorer::create("stationary", 1.0, 0.0, 0.01, 0.04).is_err());
        assert!(Explorer::create("stationary", 1.0, 0.05, -1.0, 0.04).is_err());
        assert!(Explorer::create("stationary", 1.0, 0.05, 0.01, 0.0).is_err());
        assert!(Explorer::create("stationary", -1.0, 0.05, 0.01, 0.04).is_err());
    }

    #[test]
    fn a_walkthrough_reports_each_coordinate_change() {
        let mut walk = Walkthrough::create(1.0).expect("valid seed");
        walk.advance_sample().expect("first sample");
        walk.advance_sample().expect("second sample");
        assert_eq!(walk.sample(), 2.0);
        assert_eq!(walk.num_features(), WALKTHROUGH_FEATURES);
        assert_eq!(walk.coefficients(), WALKTHROUGH_COEFFICIENTS.to_vec());
        let error = walk.error().expect("report");
        let prediction = walk.prediction().expect("report");
        assert!(close(error, walk.target() - prediction), "{error}");
        let json = walk.coordinate(0).expect("feature 0");
        assert!(json.contains("\"betaChange\""), "{json}");
        assert!(walk.coordinate(WALKTHROUGH_FEATURES).is_err());
    }

    #[test]
    fn build_information_is_available() {
        assert_eq!(core_version(), env!("CARGO_PKG_VERSION"));
        assert!(generator_id().contains("xoshiro256"), "{}", generator_id());
        assert_eq!(num_features(), NUM_FEATURES);
        assert_eq!(group_size(), GROUP_SIZE);
        assert_eq!(rolling_window(), ROLLING_WINDOW);
        assert_eq!(history_capacity(), HISTORY_CAPACITY);
        assert_eq!(event_capacity(), EVENT_CAPACITY);
        let presets = presets_json();
        for preset in Preset::ALL {
            assert!(presets.contains(preset.id()), "{presets}");
        }
    }

    /// The feature file's equation tolerance.
    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() <= 1e-12 + 1e-10 * a.abs().max(b.abs())
    }
}
