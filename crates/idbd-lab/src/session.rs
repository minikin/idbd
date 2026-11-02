//! A paired IDBD and LMS experiment that advances one shared sample at a time.

use std::collections::VecDeque;
use std::fmt;

use idbd::{Config, Idbd, Update};

use crate::lms::{Lms, LmsError};
use crate::trace::{CoordinateChange, traced_update};
use crate::world::{Event, NUM_FEATURES, Preset, World};

/// Number of recent pre-update errors in each rolling mean squared error.
pub const ROLLING_WINDOW: usize = 100;

/// Number of recent samples kept for charts. Aggregate metrics cover every sample.
pub const HISTORY_CAPACITY: usize = 2_000;

/// Number of recent world events kept for the timeline.
pub const EVENT_CAPACITY: usize = 512;

/// Initial settings of a run. Changing any of them starts a new run.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Settings {
    /// The task.
    pub preset: Preset,
    /// Seed of the task's input and event streams.
    pub seed: u64,
    /// IDBD configuration.
    pub idbd: Config,
    /// Fixed LMS step size.
    pub lms_step_size: f64,
}

/// Which learner failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Learner {
    /// The IDBD learner.
    Idbd,
    /// The LMS baseline.
    Lms,
}

/// Why a sample could not be completed by both learners.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureCause {
    /// The IDBD core rejected the sample.
    Idbd(idbd::Error),
    /// The LMS baseline rejected the sample.
    Lms(LmsError),
}

/// A sample that failed. The session stays at the last sample both learners completed
/// until it is replaced by a new one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Failure {
    /// The attempted sample index.
    pub sample: u64,
    /// The cause.
    pub cause: FailureCause,
}

impl Failure {
    /// The learner that rejected the sample.
    pub fn learner(&self) -> Learner {
        match self.cause {
            FailureCause::Idbd(_) => Learner::Idbd,
            FailureCause::Lms(_) => Learner::Lms,
        }
    }
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.cause {
            FailureCause::Idbd(e) => write!(f, "IDBD failed at sample {}: {e}", self.sample),
            FailureCause::Lms(e) => write!(f, "LMS failed at sample {}: {e}", self.sample),
        }
    }
}

impl std::error::Error for Failure {}

/// A world event with the index of the first sample it affected.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TimedEvent {
    /// The first sample generated under the changed relationship.
    pub sample: u64,
    /// The event.
    pub event: Event,
}

/// Pre-update squared errors over the most recent `ROLLING_WINDOW` samples.
#[derive(Debug, Clone)]
struct Rolling {
    values: VecDeque<f64>,
}

impl Rolling {
    fn new() -> Self {
        Self {
            values: VecDeque::with_capacity(ROLLING_WINDOW),
        }
    }

    fn push(&mut self, squared_error: f64) {
        if self.values.len() == ROLLING_WINDOW {
            self.values.pop_front();
        }
        self.values.push_back(squared_error);
    }

    /// Mean of the last `min(ROLLING_WINDOW, samples)` values.
    fn mean(&self) -> f64 {
        if self.values.is_empty() {
            return 0.0;
        }
        let sum: f64 = self.values.iter().sum();
        sum / count_as_f64(self.values.len())
    }
}

fn count_as_f64(count: usize) -> f64 {
    let count = u32::try_from(count).expect("window sizes fit in 32 bits");
    f64::from(count)
}

/// One completed sample as recorded for charts.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Record {
    /// The sample index, starting at 1.
    pub sample: u64,
    /// IDBD pre-update error.
    pub idbd_error: f64,
    /// LMS pre-update error.
    pub lms_error: f64,
    /// IDBD rolling MSE after this sample.
    pub idbd_rolling_mse: f64,
    /// LMS rolling MSE after this sample.
    pub lms_rolling_mse: f64,
}

/// Running sums over every completed sample.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Totals {
    /// Number of completed samples.
    pub samples: u64,
    /// Sum of IDBD squared pre-update errors.
    pub idbd_squared_error: f64,
    /// Sum of LMS squared pre-update errors.
    pub lms_squared_error: f64,
}

/// A paired experiment: IDBD and LMS learn from the same stream, one sample at a time.
///
/// Every view of a session refers to the last sample that both learners completed. If
/// either learner rejects a sample, neither learns from it, the session records the
/// [`Failure`], and further steps are refused.
#[derive(Debug, Clone)]
pub struct Session {
    settings: Settings,
    world: World,
    idbd: Idbd,
    lms: Lms,
    features: [f64; NUM_FEATURES],
    attempt: [f64; NUM_FEATURES],
    coefficients: [f64; NUM_FEATURES],
    target: f64,
    changes: [CoordinateChange; NUM_FEATURES],
    last_idbd: Option<Update>,
    last_lms_error: f64,
    idbd_rolling: Rolling,
    lms_rolling: Rolling,
    history: VecDeque<Record>,
    alpha_history: VecDeque<[f64; NUM_FEATURES]>,
    events: VecDeque<TimedEvent>,
    event_buffer: Vec<Event>,
    totals: Totals,
    failure: Option<Failure>,
}

/// Invalid session settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsError {
    /// The LMS step size is not finite and positive.
    LmsStepSize,
}

impl fmt::Display for SettingsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LmsStepSize => f.write_str("LMS step size must be finite and positive"),
        }
    }
}

impl std::error::Error for SettingsError {}

impl Session {
    /// Starts a run at sample 0.
    ///
    /// # Errors
    ///
    /// Returns [`SettingsError::LmsStepSize`] for an invalid LMS step size.
    pub fn new(settings: Settings) -> Result<Self, SettingsError> {
        let lms = Lms::new(NUM_FEATURES, settings.lms_step_size)
            .map_err(|_| SettingsError::LmsStepSize)?;
        let idbd = Idbd::new(NUM_FEATURES, settings.idbd)
            .expect("NUM_FEATURES is nonzero and Config is validated");
        let world = World::new(settings.preset, settings.seed);
        let mut coefficients = [0.0; NUM_FEATURES];
        coefficients.copy_from_slice(world.coefficients());
        Ok(Self {
            settings,
            world,
            idbd,
            lms,
            features: [0.0; NUM_FEATURES],
            attempt: [0.0; NUM_FEATURES],
            coefficients,
            target: 0.0,
            changes: [CoordinateChange::default(); NUM_FEATURES],
            last_idbd: None,
            last_lms_error: 0.0,
            idbd_rolling: Rolling::new(),
            lms_rolling: Rolling::new(),
            history: VecDeque::with_capacity(HISTORY_CAPACITY),
            alpha_history: VecDeque::with_capacity(HISTORY_CAPACITY),
            events: VecDeque::with_capacity(EVENT_CAPACITY),
            event_buffer: Vec::with_capacity(4),
            totals: Totals::default(),
            failure: None,
        })
    }

    /// Processes one shared sample.
    ///
    /// # Errors
    ///
    /// Returns the [`Failure`] if either learner rejects the sample, or the earlier
    /// failure if the session has already stopped. The learners and every published
    /// view stay at the last completed sample.
    pub fn step(&mut self) -> Result<(), Failure> {
        if let Some(failure) = self.failure {
            return Err(failure);
        }
        let target = self
            .world
            .next_sample(&mut self.attempt, &mut self.event_buffer);
        let sample = self.world.generated();
        let lms = match self.lms.check(&self.attempt, target) {
            Ok(outcome) => outcome,
            Err(e) => return Err(self.fail(sample, FailureCause::Lms(e))),
        };
        let report = match traced_update(&mut self.idbd, &self.attempt, target, &mut self.changes) {
            Ok(report) => report,
            Err(e) => return Err(self.fail(sample, FailureCause::Idbd(e))),
        };
        self.lms
            .update(&self.attempt, target)
            .expect("Lms::check accepted this example");
        self.commit(sample, target, report, lms.error);
        Ok(())
    }

    /// Processes up to `samples` shared samples and stops early on failure.
    ///
    /// Returns the number of completed samples.
    ///
    /// # Errors
    ///
    /// Returns the [`Failure`] that stopped the run.
    pub fn run(&mut self, samples: u64) -> Result<u64, Failure> {
        for done in 0..samples {
            if let Err(failure) = self.step() {
                return if done == 0 { Err(failure) } else { Ok(done) };
            }
        }
        Ok(samples)
    }

    fn fail(&mut self, sample: u64, cause: FailureCause) -> Failure {
        let failure = Failure { sample, cause };
        self.failure = Some(failure);
        failure
    }

    fn commit(&mut self, sample: u64, target: f64, report: Update, lms_error: f64) {
        self.features = self.attempt;
        self.coefficients.copy_from_slice(self.world.coefficients());
        self.target = target;
        self.last_idbd = Some(report);
        self.last_lms_error = lms_error;
        let (idbd_sq, lms_sq) = (report.error * report.error, lms_error * lms_error);
        self.idbd_rolling.push(idbd_sq);
        self.lms_rolling.push(lms_sq);
        self.totals.samples = sample;
        self.totals.idbd_squared_error += idbd_sq;
        self.totals.lms_squared_error += lms_sq;
        if self.history.len() == HISTORY_CAPACITY {
            self.history.pop_front();
            self.alpha_history.pop_front();
        }
        self.history.push_back(Record {
            sample,
            idbd_error: report.error,
            lms_error,
            idbd_rolling_mse: self.idbd_rolling.mean(),
            lms_rolling_mse: self.lms_rolling.mean(),
        });
        let mut alphas = [0.0; NUM_FEATURES];
        for (a, alpha) in alphas.iter_mut().zip(self.idbd.step_sizes()) {
            *a = alpha;
        }
        self.alpha_history.push_back(alphas);
        for &event in &self.event_buffer {
            if self.events.len() == EVENT_CAPACITY {
                self.events.pop_front();
            }
            self.events.push_back(TimedEvent { sample, event });
        }
    }

    /// Asks the world to move relevance to the next group before the next sample.
    ///
    /// Neither learner changes until that sample is processed.
    pub fn change_the_world(&mut self) {
        self.world.request_relevance_shift();
    }

    /// Returns `true` if a requested change waits for the next sample.
    pub fn change_pending(&self) -> bool {
        self.world.shift_pending()
    }

    /// Returns the run settings.
    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    /// Returns the last sample index completed by both learners; 0 before any sample.
    pub fn completed(&self) -> u64 {
        self.totals.samples
    }

    /// Returns the failure that stopped the session, if any.
    pub fn failure(&self) -> Option<&Failure> {
        self.failure.as_ref()
    }

    /// Returns the IDBD learner at the last completed sample.
    pub fn idbd(&self) -> &Idbd {
        &self.idbd
    }

    /// Returns the LMS learner at the last completed sample.
    pub fn lms(&self) -> &Lms {
        &self.lms
    }

    /// Returns the inputs of the last completed sample.
    pub fn features(&self) -> &[f64] {
        &self.features
    }

    /// Returns the target of the last completed sample.
    pub fn target(&self) -> f64 {
        self.target
    }

    /// Returns the true coefficients that generated the last completed sample.
    pub fn coefficients(&self) -> &[f64] {
        &self.coefficients
    }

    /// Returns the IDBD report of the last completed sample.
    pub fn last_idbd_update(&self) -> Option<&Update> {
        self.last_idbd.as_ref()
    }

    /// Returns the LMS pre-update error of the last completed sample.
    pub fn last_lms_error(&self) -> f64 {
        self.last_lms_error
    }

    /// Returns each coordinate's IDBD change on the last completed sample.
    pub fn changes(&self) -> &[CoordinateChange] {
        &self.changes
    }

    /// Returns the IDBD rolling MSE over the last `min(100, completed)` samples.
    pub fn idbd_rolling_mse(&self) -> f64 {
        self.idbd_rolling.mean()
    }

    /// Returns the LMS rolling MSE over the last `min(100, completed)` samples.
    pub fn lms_rolling_mse(&self) -> f64 {
        self.lms_rolling.mean()
    }

    /// Returns running sums over every completed sample.
    pub fn totals(&self) -> &Totals {
        &self.totals
    }

    /// Returns the chart records of up to the last 2,000 samples, oldest first.
    pub fn history(&self) -> impl ExactSizeIterator<Item = &Record> {
        self.history.iter()
    }

    /// Returns IDBD step sizes recorded after each retained sample, oldest first.
    pub fn alpha_history(&self) -> impl ExactSizeIterator<Item = &[f64; NUM_FEATURES]> {
        self.alpha_history.iter()
    }

    /// Returns up to the last 512 world events, oldest first.
    pub fn events(&self) -> impl ExactSizeIterator<Item = &TimedEvent> {
        self.events.iter()
    }
}

#[cfg(test)]
#[expect(clippy::float_cmp, reason = "tests assert exact, deterministic values")]
mod tests {
    use super::*;
    use crate::world::{GROUP_SIZE, Origin};

    fn settings(preset: Preset, seed: u64) -> Settings {
        Settings {
            preset,
            seed,
            idbd: Config::new(0.05, 0.01).expect("valid"),
            lms_step_size: 0.04,
        }
    }

    fn state_bits(session: &Session) -> Vec<u64> {
        let idbd = session.idbd();
        idbd.weights()
            .iter()
            .chain(idbd.log_step_sizes())
            .chain(idbd.traces())
            .chain(session.lms().weights())
            .map(|v| v.to_bits())
            .collect()
    }

    #[test]
    fn d03_step_processes_exactly_one_shared_sample() {
        let mut session = Session::new(settings(Preset::Sutton1992, 1)).expect("valid");
        session.run(10).expect("run");
        assert_eq!(session.completed(), 10);
        session.step().expect("step");
        assert_eq!(session.completed(), 11);
        assert_eq!(session.history().len(), 11);
        assert_eq!(session.history().last().map(|r| r.sample), Some(11));
        assert_eq!(session.totals().samples, 11);
    }

    #[test]
    fn d02_d07_both_learners_see_the_same_sample() {
        let mut session = Session::new(settings(Preset::Sutton1992, 2)).expect("valid");
        let mut world = World::new(Preset::Sutton1992, 2);
        let mut idbd = Idbd::new(NUM_FEATURES, session.settings().idbd).expect("valid");
        let mut lms = Lms::new(NUM_FEATURES, 0.04).expect("valid");
        let mut x = [0.0; NUM_FEATURES];
        let mut events = Vec::new();
        for _ in 0..300 {
            let target = world.next_sample(&mut x, &mut events);
            let a = idbd.update(&x, target).expect("idbd");
            let b = lms.update(&x, target).expect("lms");
            session.step().expect("step");
            let last = session.history().last().copied().expect("record");
            assert_eq!(session.features(), &x);
            assert_eq!(session.target(), target);
            assert_eq!(last.idbd_error.to_bits(), a.error.to_bits());
            assert_eq!(last.lms_error.to_bits(), b.error.to_bits());
        }
        assert_eq!(session.idbd().weights(), idbd.weights());
        assert_eq!(session.lms().weights(), lms.weights());
        assert_eq!(session.coefficients(), world.coefficients());
    }

    #[test]
    fn d07_rolling_mse_uses_the_last_min_100_errors() {
        let mut session = Session::new(settings(Preset::Stationary, 3)).expect("valid");
        let mut squared = Vec::new();
        for _ in 0..250 {
            session.step().expect("step");
            let last = session.history().last().copied().expect("record");
            squared.push(last.idbd_error * last.idbd_error);
            let window = &squared[squared.len().saturating_sub(100)..];
            let expected = window.iter().sum::<f64>() / count_as_f64(window.len());
            assert!((session.idbd_rolling_mse() - expected).abs() < 1e-9 * expected.max(1.0));
            assert_eq!(last.idbd_rolling_mse, session.idbd_rolling_mse());
        }
        let total: f64 = squared.iter().sum();
        assert!((session.totals().idbd_squared_error - total).abs() < 1e-9 * total);
    }

    #[test]
    fn d04_change_the_world_keeps_learner_state_until_the_next_sample() {
        let mut session = Session::new(settings(Preset::Sutton1992, 4)).expect("valid");
        session.run(200).expect("run");
        let before = state_bits(&session);
        session.change_the_world();
        assert!(session.change_pending());
        assert_eq!(state_bits(&session), before, "no learning until a sample");
        assert_eq!(session.completed(), 200);
        session.step().expect("step");
        assert!(!session.change_pending());
        let at_201: Vec<Event> = session
            .events()
            .filter(|e| e.sample == 201)
            .map(|e| e.event)
            .collect();
        assert_eq!(
            at_201.first().copied(),
            Some(Event::RelevanceShift {
                from: 0,
                to: 1,
                origin: Origin::Requested
            }),
            "requested shift applies first at sample 201"
        );
        assert!(
            session.coefficients()[..GROUP_SIZE]
                .iter()
                .all(|&c| c == 0.0)
        );
        assert_ne!(state_bits(&session), before);
    }

    #[test]
    fn d06_replay_is_independent_of_batching() {
        let mut one_by_one = Session::new(settings(Preset::ChangingRelevance, 6)).expect("valid");
        let mut batched = Session::new(settings(Preset::ChangingRelevance, 6)).expect("valid");
        for _ in 0..1_000 {
            one_by_one.step().expect("step");
        }
        batched.run(400).expect("run");
        batched.run(600).expect("run");
        assert_eq!(state_bits(&one_by_one), state_bits(&batched));
    }

    #[test]
    fn d11_history_is_bounded_and_totals_cover_every_sample() {
        let mut session = Session::new(settings(Preset::Sutton1992, 11)).expect("valid");
        session.run(2_500).expect("run");
        assert_eq!(session.history().len(), HISTORY_CAPACITY);
        assert_eq!(session.alpha_history().len(), HISTORY_CAPACITY);
        assert_eq!(session.history().next().map(|r| r.sample), Some(501));
        assert_eq!(session.totals().samples, 2_500);
        assert!(session.events().len() <= EVENT_CAPACITY);
    }

    #[test]
    fn d08_a_failure_freezes_the_last_mutually_completed_sample() {
        let mut settings = settings(Preset::Sutton1992, 8);
        settings.lms_step_size = 5.0;
        let mut session = Session::new(settings).expect("valid");
        let failure = loop {
            let before = state_bits(&session);
            let completed = session.completed();
            let features = session.features().to_vec();
            match session.step() {
                Ok(()) => assert!(session.completed() < 10_000, "LMS should diverge"),
                Err(failure) => {
                    assert_eq!(state_bits(&session), before, "no partial learning");
                    assert_eq!(session.completed(), completed);
                    assert_eq!(session.features(), features.as_slice());
                    assert_eq!(failure.sample, completed + 1);
                    break failure;
                }
            }
        };
        assert_eq!(failure.learner(), Learner::Lms);
        assert_eq!(session.step(), Err(failure), "playback requires a new run");
        assert_eq!(session.run(5), Err(failure));
        assert!(
            failure.to_string().starts_with("LMS failed at sample"),
            "{failure}"
        );
        assert!(
            session
                .history()
                .all(|r| r.idbd_error.is_finite() && r.lms_error.is_finite())
        );
    }

    #[test]
    fn invalid_lms_step_size_is_rejected() {
        let mut settings = settings(Preset::Stationary, 1);
        settings.lms_step_size = 0.0;
        assert_eq!(
            Session::new(settings).err(),
            Some(SettingsError::LmsStepSize)
        );
    }
}
