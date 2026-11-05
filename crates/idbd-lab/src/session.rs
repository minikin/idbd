//! A paired IDBD and LMS experiment that advances one shared sample at a time.

use std::collections::{BTreeSet, VecDeque};
use std::fmt;
use std::mem;

use idbd::autostep::{self, Autostep};
use idbd::{Config, Idbd, Update};

use crate::lms::{Lms, LmsError};
use crate::trace::{CoordinateChange, traced_update};
use crate::world::{Event, NUM_FEATURES, Origin, Preset, World};

/// Number of recent pre-update errors in each rolling mean squared error.
pub const ROLLING_WINDOW: usize = 100;

/// Number of recent samples kept for charts. Aggregate metrics cover every sample.
pub const HISTORY_CAPACITY: usize = 2_000;

/// Number of recent world events kept for the timeline.
pub const EVENT_CAPACITY: usize = 512;

/// Largest number of requested changes that can wait for a later sample.
///
/// A replayed schedule comes from a file the user loads, so its length is not trusted.
pub const SCHEDULE_CAPACITY: usize = 10_000;

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
    /// Autostep configuration.
    pub autostep: autostep::Config,
}

/// Which learner failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Learner {
    /// The IDBD learner.
    Idbd,
    /// The LMS baseline.
    Lms,
    /// The Autostep learner.
    Autostep,
}

/// Why a sample could not be completed by both learners.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureCause {
    /// The IDBD core rejected the sample.
    Idbd(idbd::Error),
    /// The LMS baseline rejected the sample.
    Lms(LmsError),
    /// The Autostep learner rejected the sample.
    Autostep(idbd::Error),
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
            FailureCause::Autostep(_) => Learner::Autostep,
        }
    }
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.cause {
            FailureCause::Idbd(e) => write!(f, "IDBD failed at sample {}: {e}", self.sample),
            FailureCause::Lms(e) => write!(f, "LMS failed at sample {}: {e}", self.sample),
            FailureCause::Autostep(e) => {
                write!(f, "Autostep failed at sample {}: {e}", self.sample)
            }
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

    /// Mean of the last `min(ROLLING_WINDOW, samples)` values, or `None` before any.
    fn mean(&self) -> Option<f64> {
        if self.values.is_empty() {
            return None;
        }
        let sum: f64 = self.values.iter().sum();
        Some(sum / count_as_f64(self.values.len()))
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
    /// Autostep pre-update error.
    pub autostep_error: f64,
    /// Autostep rolling MSE after this sample.
    pub autostep_rolling_mse: f64,
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
    /// Sum of Autostep squared pre-update errors.
    pub autostep_squared_error: f64,
}

/// A shared experiment: IDBD, LMS and Autostep learn from the same stream, one sample at
/// a time.
///
/// Every view of a session refers to the last sample that every learner completed. If
/// any learner rejects a sample, none learns from it, the session records the
/// [`Failure`], and further steps are refused. The learners are checked in the order
/// LMS, IDBD, Autostep, and the first rejection is the one reported.
#[derive(Debug, Clone)]
pub struct Session {
    settings: Settings,
    world: World,
    idbd: Idbd,
    lms: Lms,
    autostep: Autostep,
    /// Learners update these copies first, so a sample that one learner rejects leaves
    /// the others unchanged. `clone_from` refreshes them without allocating.
    idbd_scratch: Idbd,
    autostep_scratch: Autostep,
    features: [f64; NUM_FEATURES],
    attempt: [f64; NUM_FEATURES],
    coefficients: [f64; NUM_FEATURES],
    target: f64,
    changes: [CoordinateChange; NUM_FEATURES],
    attempt_changes: [CoordinateChange; NUM_FEATURES],
    last_idbd: Option<Update>,
    last_lms_error: Option<f64>,
    last_autostep_error: Option<f64>,
    idbd_rolling: Rolling,
    lms_rolling: Rolling,
    autostep_rolling: Rolling,
    history: VecDeque<Record>,
    alpha_history: VecDeque<[f64; NUM_FEATURES]>,
    events: VecDeque<TimedEvent>,
    event_buffer: Vec<Event>,
    schedule: BTreeSet<u64>,
    requested: Vec<u64>,
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

/// A requested change that [`Session::schedule_change`] cannot accept.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScheduleError {
    /// The sample is already completed, so a change cannot apply before it.
    NotInFuture {
        /// The requested sample.
        sample: u64,
        /// The last completed sample.
        completed: u64,
    },
    /// [`SCHEDULE_CAPACITY`] changes already wait.
    Full,
}

impl fmt::Display for ScheduleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotInFuture { sample, completed } => write!(
                f,
                "a change cannot apply before sample {sample} because sample {completed} is already completed"
            ),
            Self::Full => write!(f, "at most {SCHEDULE_CAPACITY} changes can wait"),
        }
    }
}

impl std::error::Error for ScheduleError {}

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
        let autostep = Autostep::new(NUM_FEATURES, settings.autostep)
            .expect("NUM_FEATURES is nonzero and autostep::Config is validated");
        let world = World::new(settings.preset, settings.seed);
        let mut coefficients = [0.0; NUM_FEATURES];
        coefficients.copy_from_slice(world.coefficients());
        Ok(Self {
            settings,
            world,
            idbd_scratch: idbd.clone(),
            autostep_scratch: autostep.clone(),
            idbd,
            lms,
            autostep,
            features: [0.0; NUM_FEATURES],
            attempt: [0.0; NUM_FEATURES],
            coefficients,
            target: 0.0,
            changes: [CoordinateChange::default(); NUM_FEATURES],
            attempt_changes: [CoordinateChange::default(); NUM_FEATURES],
            last_idbd: None,
            last_lms_error: None,
            last_autostep_error: None,
            idbd_rolling: Rolling::new(),
            lms_rolling: Rolling::new(),
            autostep_rolling: Rolling::new(),
            history: VecDeque::with_capacity(HISTORY_CAPACITY),
            alpha_history: VecDeque::with_capacity(HISTORY_CAPACITY),
            events: VecDeque::with_capacity(EVENT_CAPACITY),
            event_buffer: Vec::with_capacity(4),
            schedule: BTreeSet::new(),
            requested: Vec::new(),
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
        let next = self.world.generated() + 1;
        if self.schedule.first() == Some(&next) {
            self.schedule.pop_first();
            self.world.request_relevance_shift();
        }
        let target = self
            .world
            .next_sample(&mut self.attempt, &mut self.event_buffer);
        let sample = self.world.generated();
        let lms = match self.lms.check(&self.attempt, target) {
            Ok(outcome) => outcome,
            Err(e) => return Err(self.fail(sample, FailureCause::Lms(e))),
        };
        // Traced into a scratch buffer so a rejected sample cannot leave its inputs in the
        // published per-coordinate view beside the last completed sample's weights.
        self.idbd_scratch.clone_from(&self.idbd);
        let report = match traced_update(
            &mut self.idbd_scratch,
            &self.attempt,
            target,
            &mut self.attempt_changes,
        ) {
            Ok(report) => report,
            Err(e) => return Err(self.fail(sample, FailureCause::Idbd(e))),
        };
        self.autostep_scratch.clone_from(&self.autostep);
        let autostep_error = match self.autostep_scratch.update(&self.attempt, target) {
            Ok(update) => update.error,
            Err(e) => return Err(self.fail(sample, FailureCause::Autostep(e))),
        };
        mem::swap(&mut self.idbd, &mut self.idbd_scratch);
        mem::swap(&mut self.autostep, &mut self.autostep_scratch);
        self.lms
            .update(&self.attempt, target)
            .expect("Lms::check accepted this example");
        self.commit(sample, target, report, lms.error, autostep_error);
        Ok(())
    }

    /// Processes up to `samples` shared samples and stops early on failure.
    ///
    /// Returns the number of completed samples, which is less than `samples` when a
    /// failure stopped the run after some progress. [`Session::failure`] then holds the
    /// cause.
    ///
    /// # Errors
    ///
    /// Returns the [`Failure`] only when the first attempted sample fails, so that a
    /// caller who got no progress at all does not have to ask why.
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

    fn commit(
        &mut self,
        sample: u64,
        target: f64,
        report: Update,
        lms_error: f64,
        autostep_error: f64,
    ) {
        self.features = self.attempt;
        self.changes = self.attempt_changes;
        self.coefficients.copy_from_slice(self.world.coefficients());
        self.target = target;
        self.last_idbd = Some(report);
        self.last_lms_error = Some(lms_error);
        self.last_autostep_error = Some(autostep_error);
        let (idbd_sq, lms_sq) = (report.error * report.error, lms_error * lms_error);
        self.idbd_rolling.push(idbd_sq);
        self.lms_rolling.push(lms_sq);
        let autostep_sq = autostep_error * autostep_error;
        self.autostep_rolling.push(autostep_sq);
        self.totals.samples = sample;
        self.totals.idbd_squared_error += idbd_sq;
        self.totals.lms_squared_error += lms_sq;
        self.totals.autostep_squared_error += autostep_sq;
        if self.history.len() == HISTORY_CAPACITY {
            self.history.pop_front();
            self.alpha_history.pop_front();
        }
        let pushed = "a squared error was just pushed";
        self.history.push_back(Record {
            sample,
            idbd_error: report.error,
            lms_error,
            idbd_rolling_mse: self.idbd_rolling.mean().expect(pushed),
            lms_rolling_mse: self.lms_rolling.mean().expect(pushed),
            autostep_error,
            autostep_rolling_mse: self.autostep_rolling.mean().expect(pushed),
        });
        let mut alphas = [0.0; NUM_FEATURES];
        for (a, alpha) in alphas.iter_mut().zip(self.idbd.step_sizes()) {
            *a = alpha;
        }
        self.alpha_history.push_back(alphas);
        for &event in &self.event_buffer {
            if matches!(
                event,
                Event::RelevanceShift {
                    origin: Origin::Requested,
                    ..
                }
            ) {
                self.requested.push(sample);
            }
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
    ///
    /// Changes scheduled for later samples through [`Session::schedule_change`] do not
    /// count until that sample is attempted.
    pub fn change_pending(&self) -> bool {
        self.world.shift_pending()
    }

    /// Schedules the same change as [`Session::change_the_world`] to apply before
    /// `sample`.
    ///
    /// Replaying the samples from [`Session::requested_changes`] on a new session with
    /// the same settings repeats the run. Scheduling one sample twice still produces one
    /// change.
    ///
    /// # Errors
    ///
    /// Returns [`ScheduleError::NotInFuture`] if `sample` is already completed, and
    /// [`ScheduleError::Full`] if [`SCHEDULE_CAPACITY`] other changes already wait.
    pub fn schedule_change(&mut self, sample: u64) -> Result<(), ScheduleError> {
        let completed = self.completed();
        if sample <= completed {
            return Err(ScheduleError::NotInFuture { sample, completed });
        }
        if self.schedule.len() >= SCHEDULE_CAPACITY && !self.schedule.contains(&sample) {
            return Err(ScheduleError::Full);
        }
        self.schedule.insert(sample);
        Ok(())
    }

    /// Returns the samples that scheduled changes still wait for, in order.
    pub fn scheduled_changes(&self) -> impl ExactSizeIterator<Item = u64> + '_ {
        self.schedule.iter().copied()
    }

    /// Returns every sample that a requested change applied before, in order.
    ///
    /// Unlike [`Session::events`], this record is complete. It grows by at most one
    /// entry per completed sample.
    pub fn requested_changes(&self) -> &[u64] {
        &self.requested
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

    /// Returns the Autostep learner at the last completed sample.
    pub fn autostep(&self) -> &Autostep {
        &self.autostep
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

    /// Returns the LMS pre-update error of the last completed sample, or `None` before
    /// the first one.
    pub fn last_lms_error(&self) -> Option<f64> {
        self.last_lms_error
    }

    /// Returns the Autostep pre-update error of the last completed sample, or `None`
    /// before the first one.
    pub fn last_autostep_error(&self) -> Option<f64> {
        self.last_autostep_error
    }

    /// Returns each coordinate's IDBD change on the last completed sample.
    pub fn changes(&self) -> &[CoordinateChange] {
        &self.changes
    }

    /// Returns the IDBD rolling MSE over the last `min(100, completed)` samples, or
    /// `None` before the first one.
    pub fn idbd_rolling_mse(&self) -> Option<f64> {
        self.idbd_rolling.mean()
    }

    /// Returns the LMS rolling MSE over the last `min(100, completed)` samples, or
    /// `None` before the first one.
    pub fn lms_rolling_mse(&self) -> Option<f64> {
        self.lms_rolling.mean()
    }

    /// Returns the Autostep rolling MSE over the last `min(ROLLING_WINDOW, completed)`
    /// pre-update errors, or `None` before the first sample.
    pub fn autostep_rolling_mse(&self) -> Option<f64> {
        self.autostep_rolling.mean()
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
    use crate::world::GROUP_SIZE;

    fn settings(preset: Preset, seed: u64) -> Settings {
        Settings {
            preset,
            seed,
            idbd: Config::new(0.05, 0.01).expect("valid"),
            lms_step_size: 0.04,
            autostep: autostep::Config::new(0.005).expect("valid"),
        }
    }

    fn state_bits(session: &Session) -> Vec<u64> {
        let idbd = session.idbd();
        idbd.weights()
            .iter()
            .chain(idbd.log_step_sizes())
            .chain(idbd.traces())
            .chain(session.lms().weights())
            .chain(session.autostep().weights())
            .chain(session.autostep().step_sizes())
            .chain(session.autostep().traces())
            .chain(session.autostep().normalizers())
            .map(|v| v.to_bits())
            .collect()
    }

    #[test]
    fn d08_an_autostep_failure_leaves_every_learner_at_the_last_sample() {
        let mut settings = settings(Preset::Sutton1992, 5);
        settings.autostep = autostep::Config::new(0.005)
            .and_then(|c| c.with_meta_step_size(1e6))
            .expect("valid");
        let mut session = Session::new(settings).expect("valid");
        session
            .step()
            .expect("every trace is zero, so no step size adapts yet");
        let before = state_bits(&session);
        let errors = (
            session.last_lms_error(),
            session.last_autostep_error(),
            session.idbd_rolling_mse(),
        );
        let failure = session.step().expect_err("exp(1e6 * g / v) overflows");
        assert_eq!(failure.learner(), Learner::Autostep);
        assert_eq!(failure.sample, 2);
        assert!(
            failure.to_string().starts_with(
                "Autostep failed at sample 2: numerical failure at the step size stage"
            ),
            "{failure}"
        );
        assert_eq!(
            state_bits(&session),
            before,
            "no learner learned from sample 2"
        );
        assert_eq!(session.completed(), 1);
        assert_eq!(
            (
                session.last_lms_error(),
                session.last_autostep_error(),
                session.idbd_rolling_mse()
            ),
            errors
        );
        assert_eq!(session.step(), Err(failure), "further steps are refused");
    }

    #[test]
    fn d07_autostep_is_scored_on_the_same_stream() {
        let mut session = Session::new(settings(Preset::Sutton1992, 12)).expect("valid");
        assert_eq!(session.last_autostep_error(), None);
        assert_eq!(session.autostep_rolling_mse(), None);
        session.run(150).expect("run");
        let mut learner = Autostep::new(NUM_FEATURES, session.settings().autostep).expect("valid");
        let mut world = World::new(Preset::Sutton1992, 12);
        let (mut x, mut events) = ([0.0; NUM_FEATURES], Vec::new());
        let mut squared = Vec::new();
        for _ in 0..150 {
            let target = world.next_sample(&mut x, &mut events);
            let error = learner.update(&x, target).expect("finite").error;
            squared.push(error * error);
        }
        assert_eq!(session.autostep().snapshot(), learner.snapshot());
        let last = session.history().last().copied().expect("history");
        assert_eq!(Some(last.autostep_error), session.last_autostep_error());
        let window = &squared[squared.len() - ROLLING_WINDOW..];
        let expected = window.iter().sum::<f64>()
            / crate::stats::exact_f64(u64::try_from(window.len()).expect("small"));
        let rolling = session.autostep_rolling_mse().expect("samples completed");
        assert!((rolling - expected).abs() < 1e-12 * expected.max(1.0));
        assert_eq!(last.autostep_rolling_mse, rolling);
        let total: f64 = squared.iter().sum();
        assert!((session.totals().autostep_squared_error - total).abs() < 1e-9 * total);
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
    fn a_fresh_session_reports_no_measurement_yet() {
        let session = Session::new(settings(Preset::Stationary, 1)).expect("valid");
        assert_eq!(session.completed(), 0);
        assert_eq!(session.last_idbd_update(), None);
        assert_eq!(session.last_lms_error(), None);
        assert_eq!(session.idbd_rolling_mse(), None, "no window to average");
        assert_eq!(session.lms_rolling_mse(), None, "no window to average");
    }

    #[test]
    fn d08_a_failed_idbd_update_leaves_the_coordinate_view_at_the_last_sample() {
        let mut settings = settings(Preset::Sutton1992, 81);
        settings.idbd = Config::new(0.05, 1e6).expect("valid");
        settings.lms_step_size = 0.001;
        let mut session = Session::new(settings).expect("valid");
        let mut stopped = None;
        for _ in 0..10_000_u32 {
            let changes = session.changes().to_vec();
            let features = session.features().to_vec();
            if let Err(failure) = session.step() {
                assert_eq!(failure.learner(), Learner::Idbd, "{failure}");
                assert_eq!(
                    session.changes(),
                    changes.as_slice(),
                    "a rejected sample publishes no coordinate change"
                );
                assert_eq!(session.features(), features.as_slice());
                stopped = Some(failure);
                break;
            }
            let inputs: Vec<f64> = session.changes().iter().map(|c| c.input).collect();
            assert_eq!(
                inputs.as_slice(),
                session.features(),
                "the coordinate view describes the completed sample"
            );
        }
        let failure = stopped.expect("a meta step size of 1e6 diverges within 10,000 samples");
        assert!(
            failure.to_string().starts_with("IDBD failed at sample"),
            "{failure}"
        );
    }

    #[test]
    fn run_reports_the_samples_it_completed_before_a_failure() {
        let mut settings = settings(Preset::Sutton1992, 8);
        settings.lms_step_size = 5.0;
        let mut session = Session::new(settings).expect("valid");
        let completed = session.run(10_000).expect("the first sample completes");
        assert!(
            completed > 0 && completed < 10_000,
            "stopped at {completed}"
        );
        assert_eq!(completed, session.completed());
        let failure = session.failure().copied().expect("the cause is recorded");
        assert_eq!(failure.sample, completed + 1);
        assert_eq!(
            session.run(5),
            Err(failure),
            "a stopped session refuses more"
        );
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
            assert_eq!(
                session.last_lms_error().map(f64::to_bits),
                Some(b.error.to_bits())
            );
            assert_eq!(session.last_idbd_update().copied(), Some(a));
        }
        assert_eq!(session.idbd().weights(), idbd.weights());
        assert_eq!(session.lms().weights(), lms.weights());
        assert_eq!(session.coefficients(), world.coefficients());
        assert!(session.failure().is_none(), "{:?}", session.failure());
        let changes = session.changes();
        assert_eq!(changes.len(), NUM_FEATURES);
        for (change, (&w, &feature)) in changes.iter().zip(idbd.weights().iter().zip(&x)) {
            assert_eq!(change.new_weight, w, "{change:?}");
            assert_eq!(change.input, feature, "{change:?}");
        }
    }

    #[test]
    fn d07_both_learners_report_their_own_squared_errors() {
        let mut session = Session::new(settings(Preset::Stationary, 23)).expect("valid");
        let mut idbd_squared = Vec::new();
        let mut lms_squared = Vec::new();
        for _ in 0..3 {
            session.step().expect("step");
            let last = session.history().last().copied().expect("record");
            idbd_squared.push(last.idbd_error * last.idbd_error);
            lms_squared.push(last.lms_error * last.lms_error);
        }
        // Three samples, so the rolling window holds all of them and the divisor is 3.
        let idbd_mean = idbd_squared.iter().sum::<f64>() / 3.0;
        let lms_mean = lms_squared.iter().sum::<f64>() / 3.0;
        assert!(
            lms_mean > 0.0 && (lms_mean - idbd_mean).abs() > 1e-12,
            "the learners differ: idbd {idbd_mean} vs lms {lms_mean}"
        );
        assert_eq!(
            session.idbd_rolling_mse().map(f64::to_bits),
            Some(idbd_mean.to_bits())
        );
        assert_eq!(
            session.lms_rolling_mse().map(f64::to_bits),
            Some(lms_mean.to_bits())
        );
        let totals = session.totals();
        assert_eq!(
            totals.idbd_squared_error.to_bits(),
            idbd_squared.iter().sum::<f64>().to_bits()
        );
        assert_eq!(
            totals.lms_squared_error.to_bits(),
            lms_squared.iter().sum::<f64>().to_bits()
        );
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
            let length = u32::try_from(window.len()).expect("at most 100");
            let expected = window.iter().sum::<f64>() / f64::from(length);
            let rolling = session.idbd_rolling_mse().expect("a sample completed");
            assert!((rolling - expected).abs() < 1e-9 * expected.max(1.0));
            assert_eq!(last.idbd_rolling_mse, rolling);
        }
        let total: f64 = squared.iter().sum();
        assert!((session.totals().idbd_squared_error - total).abs() < 1e-9 * total);
    }

    #[test]
    fn d06_a_replayed_schedule_repeats_the_run() {
        let mut original = Session::new(settings(Preset::Sutton1992, 6)).expect("valid");
        original.run(150).expect("run");
        original.change_the_world();
        original.change_the_world();
        original.run(400).expect("run");
        original.change_the_world();
        original.run(450).expect("run");
        assert_eq!(original.requested_changes(), &[151, 551]);

        let mut replay = Session::new(settings(Preset::Sutton1992, 6)).expect("valid");
        for &sample in original.requested_changes() {
            replay.schedule_change(sample).expect("future sample");
        }
        assert_eq!(replay.scheduled_changes().collect::<Vec<_>>(), [151, 551]);
        assert!(
            !replay.change_pending(),
            "a later change is not pending yet"
        );
        replay.run(1000).expect("run");

        assert_eq!(state_bits(&replay), state_bits(&original));
        assert_eq!(replay.requested_changes(), original.requested_changes());
        assert_eq!(replay.scheduled_changes().len(), 0);
        let events = |s: &Session| s.events().copied().collect::<Vec<_>>();
        assert_eq!(events(&replay), events(&original));
    }

    #[test]
    fn d06_incremental_replay_can_exceed_the_pending_schedule_capacity() {
        let mut settings = settings(Preset::Sutton1992, 1);
        settings.lms_step_size = 0.03;
        let mut original = Session::new(settings).expect("valid");
        for _ in 0..=SCHEDULE_CAPACITY {
            original.change_the_world();
            original.step().expect("completed sample");
        }
        assert_eq!(original.requested_changes().len(), SCHEDULE_CAPACITY + 1);

        let mut replay = Session::new(settings).expect("valid");
        for &sample in original.requested_changes() {
            replay.schedule_change(sample).expect("one future change");
            replay
                .run(sample - replay.completed())
                .expect("completed sample");
        }
        assert_eq!(state_bits(&replay), state_bits(&original));
        assert_eq!(replay.requested_changes(), original.requested_changes());
        assert_eq!(replay.totals(), original.totals());
        assert_eq!(
            replay.history().collect::<Vec<_>>(),
            original.history().collect::<Vec<_>>()
        );
    }

    #[test]
    fn d06_a_change_scheduled_for_the_next_sample_applies_to_it() {
        let mut session = Session::new(settings(Preset::Stationary, 2)).expect("valid");
        session.run(10).expect("run");
        session.schedule_change(11).expect("next sample");
        session.schedule_change(11).expect("a repeat merges");
        session.step().expect("step");
        assert_eq!(session.requested_changes(), &[11]);
        assert_eq!(session.events().len(), 1);
    }

    #[test]
    fn d06_a_change_cannot_be_scheduled_for_a_completed_sample() {
        let mut session = Session::new(settings(Preset::Stationary, 2)).expect("valid");
        assert_eq!(
            session.schedule_change(0),
            Err(ScheduleError::NotInFuture {
                sample: 0,
                completed: 0
            })
        );
        session.run(10).expect("run");
        let err = session.schedule_change(10).expect_err("completed");
        assert_eq!(
            err,
            ScheduleError::NotInFuture {
                sample: 10,
                completed: 10
            }
        );
        assert_eq!(
            err.to_string(),
            "a change cannot apply before sample 10 because sample 10 is already completed"
        );
        assert_eq!(session.scheduled_changes().len(), 0);
    }

    #[test]
    fn d06_the_schedule_is_bounded() {
        let mut session = Session::new(settings(Preset::Stationary, 2)).expect("valid");
        let last = u64::try_from(SCHEDULE_CAPACITY).expect("fits");
        for sample in 1..=last {
            session.schedule_change(sample).expect("below capacity");
        }
        assert_eq!(session.schedule_change(last + 1), Err(ScheduleError::Full));
        assert_eq!(
            session.schedule_change(last),
            Ok(()),
            "a repeat still merges"
        );
        assert_eq!(
            ScheduleError::Full.to_string(),
            "at most 10000 changes can wait"
        );
        assert_eq!(session.scheduled_changes().len(), SCHEDULE_CAPACITY);
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
        // Bounded so that a session which never fails, or never advances, fails the test
        // instead of looping.
        let mut stopped = None;
        for _ in 0..10_000_u32 {
            let before = state_bits(&session);
            let completed = session.completed();
            let features = session.features().to_vec();
            if let Err(failure) = session.step() {
                assert_eq!(state_bits(&session), before, "no partial learning");
                assert_eq!(session.completed(), completed);
                assert_eq!(session.features(), features.as_slice());
                assert_eq!(failure.sample, completed + 1);
                stopped = Some(failure);
                break;
            }
            assert_eq!(
                session.completed(),
                completed + 1,
                "a step advances one sample"
            );
        }
        let failure = stopped.expect("an LMS step size of 5.0 diverges within 10,000 samples");
        assert_eq!(session.failure(), Some(&failure));
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
        assert_eq!(
            SettingsError::LmsStepSize.to_string(),
            "LMS step size must be finite and positive"
        );
    }
}
