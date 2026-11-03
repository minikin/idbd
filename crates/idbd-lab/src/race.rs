//! The race on the story page: IDBD and a fixed-step LMS learn the same house, one
//! reading at a time.
//!
//! Both learners use the parameters that scenario E06 selected on the tuning seeds
//! (`docs/results/house/README.md`), so the fixed-step learner is as well tuned for the
//! house as IDBD is.

use idbd::{Config, Idbd};

use crate::evaluation::IDBD_INITIAL_STEP_SIZE;
use crate::house::{House, NUM_SENSORS, RELEVANT};
use crate::lms::Lms;

/// IDBD's meta step size, selected by scenario E06 on the switching house.
pub const RACE_THETA: f64 = 0.015;

/// The fixed LMS step size, selected by scenario E06 on the switching house.
pub const RACE_LMS_STEP_SIZE: f64 = 0.06;

/// The fixed LMS step size, selected by scenario E06 on the still house.
pub const STILL_RACE_LMS_STEP_SIZE: f64 = 0.001;

/// IDBD and a fixed-step LMS learning one house.
#[derive(Debug, Clone)]
pub struct Race {
    house: House,
    idbd: Idbd,
    lms: Lms,
    features: [f64; NUM_SENSORS],
    target: f64,
    idbd_prediction: Option<f64>,
    lms_prediction: Option<f64>,
    switched: Vec<usize>,
    failure: Option<String>,
}

impl Race {
    /// Starts a race on the switching house with this seed, before any reading.
    pub fn new(seed: u64) -> Self {
        Self::on(House::new(seed), RACE_LMS_STEP_SIZE)
    }

    /// Starts a race on the still house, where nothing switches, with the LMS step size
    /// E06 selected for it.
    pub fn still(seed: u64) -> Self {
        Self::on(House::still(seed), STILL_RACE_LMS_STEP_SIZE)
    }

    fn on(house: House, lms_step_size: f64) -> Self {
        let config = Config::new(IDBD_INITIAL_STEP_SIZE, RACE_THETA)
            .expect("the race constants form a valid IDBD configuration");
        Self {
            house,
            idbd: Idbd::new(NUM_SENSORS, config).expect("the house has sensors"),
            lms: Lms::new(NUM_SENSORS, lms_step_size).expect("the race LMS step size is positive"),
            features: [0.0; NUM_SENSORS],
            target: 0.0,
            idbd_prediction: None,
            lms_prediction: None,
            switched: Vec::new(),
            failure: None,
        }
    }

    /// Lets both learners learn from the next reading, or neither.
    ///
    /// # Errors
    ///
    /// Returns the failure message if either learner rejects the reading, or if an
    /// earlier failure already stopped the race. Neither learner changes on a failure.
    pub fn step(&mut self) -> Result<(), String> {
        if let Some(failure) = &self.failure {
            return Err(failure.clone());
        }
        let mut features = [0.0; NUM_SENSORS];
        let target = self.house.next_sample(&mut features, &mut self.switched);
        let checked = self
            .lms
            .check(&features, target)
            .map_err(|e| format!("LMS: {e}"))
            .and_then(|_| {
                self.idbd
                    .update(&features, target)
                    .map_err(|e| format!("IDBD: {e}"))
            });
        match checked {
            Ok(update) => {
                let outcome = self
                    .lms
                    .update(&features, target)
                    .expect("a successful check guarantees the update");
                self.features = features;
                self.target = target;
                self.idbd_prediction = Some(update.prediction);
                self.lms_prediction = Some(outcome.prediction);
                Ok(())
            }
            Err(message) => {
                let failure = format!("reading {}: {message}", self.house.generated());
                self.failure = Some(failure.clone());
                Err(failure)
            }
        }
    }

    /// Steps up to `readings` times and returns the number completed.
    ///
    /// # Errors
    ///
    /// Returns the failure message if the first attempted reading fails. A failure after
    /// some progress returns the number completed instead.
    pub fn run(&mut self, readings: u32) -> Result<u32, String> {
        for done in 0..readings {
            if let Err(message) = self.step() {
                return if done == 0 { Err(message) } else { Ok(done) };
            }
        }
        Ok(readings)
    }

    /// Returns the number of readings both learners completed.
    pub fn completed(&self) -> u64 {
        if self.failure.is_some() {
            self.house.generated().saturating_sub(1)
        } else {
            self.house.generated()
        }
    }

    /// Returns the sensor values of the last completed reading.
    pub fn features(&self) -> &[f64] {
        &self.features
    }

    /// Returns the temperature change of the last completed reading.
    pub fn target(&self) -> f64 {
        self.target
    }

    /// Returns IDBD's prediction for the last completed reading, made before learning.
    pub fn idbd_prediction(&self) -> Option<f64> {
        self.idbd_prediction
    }

    /// Returns LMS's prediction for the last completed reading, made before learning.
    pub fn lms_prediction(&self) -> Option<f64> {
        self.lms_prediction
    }

    /// Returns the fixed LMS step size.
    pub fn lms_step_size(&self) -> f64 {
        self.lms.step_size()
    }

    /// Returns IDBD's step size for every sensor.
    pub fn idbd_step_sizes(&self) -> Vec<f64> {
        self.idbd.step_sizes().collect()
    }

    /// Returns IDBD's weights.
    pub fn idbd_weights(&self) -> &[f64] {
        self.idbd.weights()
    }

    /// Returns LMS's weights.
    pub fn lms_weights(&self) -> &[f64] {
        self.lms.weights()
    }

    /// Returns the influences that generated the last reading.
    pub fn coefficients(&self) -> [f64; NUM_SENSORS] {
        self.house.coefficients()
    }

    /// Returns, for each relevant sensor, whether its influence is strong right now.
    pub fn strong(&self) -> [bool; RELEVANT] {
        std::array::from_fn(|sensor| self.house.is_strong(sensor))
    }

    /// Returns the relevant sensors that switched before the last reading.
    pub fn switched(&self) -> &[usize] {
        &self.switched
    }

    /// Returns the message of the failure that stopped the race, if any.
    pub fn failure(&self) -> Option<&str> {
        self.failure.as_deref()
    }
}

#[cfg(test)]
#[expect(clippy::float_cmp, reason = "tests assert exact, deterministic values")]
mod tests {
    use super::*;

    #[test]
    fn a_new_race_has_no_predictions() {
        let race = Race::new(1);
        assert_eq!(race.completed(), 0);
        assert_eq!(race.idbd_prediction(), None);
        assert_eq!(race.lms_prediction(), None);
        assert!(
            race.idbd_step_sizes()
                .iter()
                .all(|&a| (a - IDBD_INITIAL_STEP_SIZE).abs() < 1e-15),
            "IDBD stores ln(alpha), so the round trip may differ in the last bit"
        );
        assert!(race.failure().is_none());
    }

    #[test]
    fn both_learners_see_the_same_readings_as_the_house() {
        let mut race = Race::new(3);
        let mut house = House::new(3);
        let mut x = [0.0; NUM_SENSORS];
        let mut switched = Vec::new();
        for _ in 0..200 {
            race.step().expect("finite readings");
            let target = house.next_sample(&mut x, &mut switched);
            assert_eq!(race.features(), &x);
            assert_eq!(race.target(), target);
            assert_eq!(race.switched(), switched.as_slice());
            assert_eq!(race.coefficients(), house.coefficients());
        }
        assert_eq!(race.completed(), 200);
    }

    #[test]
    fn predictions_match_standalone_learners() {
        let mut race = Race::new(5);
        let config = Config::new(IDBD_INITIAL_STEP_SIZE, RACE_THETA).expect("valid");
        let mut idbd = Idbd::new(NUM_SENSORS, config).expect("valid");
        let mut lms = Lms::new(NUM_SENSORS, RACE_LMS_STEP_SIZE).expect("valid");
        let mut house = House::new(5);
        let mut x = [0.0; NUM_SENSORS];
        let mut switched = Vec::new();
        for _ in 0..500 {
            race.step().expect("finite readings");
            let target = house.next_sample(&mut x, &mut switched);
            let i = idbd.update(&x, target).expect("finite");
            let l = lms.update(&x, target).expect("finite");
            assert_eq!(race.idbd_prediction(), Some(i.prediction));
            assert_eq!(race.lms_prediction(), Some(l.prediction));
        }
        assert_eq!(race.idbd_weights(), idbd.weights());
        assert_eq!(race.lms_weights(), lms.weights());
        assert_eq!(
            race.idbd_step_sizes(),
            idbd.step_sizes().collect::<Vec<_>>()
        );
    }

    #[test]
    fn idbd_learns_to_trust_the_relevant_sensors() {
        let mut race = Race::new(7);
        assert_eq!(race.run(20_000), Ok(20_000));
        let alphas = race.idbd_step_sizes();
        let slowest_relevant = alphas[..RELEVANT]
            .iter()
            .copied()
            .fold(f64::INFINITY, f64::min);
        let fastest_inert = alphas[RELEVANT..].iter().copied().fold(0.0, f64::max);
        assert!(
            slowest_relevant > 3.0 * fastest_inert,
            "relevant {:?} vs inert {:?}",
            &alphas[..RELEVANT],
            &alphas[RELEVANT..]
        );
        assert_eq!(race.strong().len(), RELEVANT);
    }

    #[test]
    fn a_failure_stops_the_race_and_changes_nothing() {
        let mut race = Race::new(9);
        // A step size this large overflows LMS's weights within a few readings.
        race.lms = Lms::new(NUM_SENSORS, 1e300).expect("positive");
        let mut completed = 0_u64;
        let message = loop {
            let before = race.idbd_weights().to_vec();
            match race.step() {
                Ok(()) => completed += 1,
                Err(message) => {
                    assert_eq!(race.idbd_weights(), before.as_slice(), "IDBD unchanged");
                    break message;
                }
            }
            assert!(completed < 10, "LMS should diverge within a few readings");
        };
        assert!(message.contains(": LMS:"), "{message}");
        assert_eq!(race.completed(), completed);
        assert_eq!(race.failure(), Some(message.as_str()));
        assert_eq!(race.step(), Err(message.clone()));
        assert_eq!(race.run(5), Err(message));
    }

    #[test]
    fn a_still_race_never_switches_and_uses_its_own_lms_step_size() {
        let mut race = Race::still(6);
        assert_eq!(race.lms_step_size(), STILL_RACE_LMS_STEP_SIZE);
        assert_eq!(Race::new(6).lms_step_size(), RACE_LMS_STEP_SIZE);
        for _ in 0..2_000 {
            race.step().expect("finite readings");
            assert!(race.switched().is_empty());
        }
        assert_eq!(race.strong(), [true; RELEVANT]);
    }

    #[test]
    fn run_reports_partial_progress() {
        let mut race = Race::new(2);
        assert_eq!(race.run(0), Ok(0));
        assert_eq!(race.run(3), Ok(3));
        assert_eq!(race.completed(), 3);
    }
}
