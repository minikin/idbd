//! The story page's race: IDBD and a tuned fixed-step LMS learning the thermostat house.
//!
//! Every learning value comes from `idbd_lab::race::Race`, which drives the `idbd`
//! core. The visitor's own guess is a plain dot product of their slider values with the
//! readings this type returns, computed on the page.

use idbd_lab::evaluation::IDBD_INITIAL_STEP_SIZE;
use idbd_lab::house::{LEVELS, NOISE, NUM_SENSORS, RELEVANT, SENSORS, SWITCH_PROBABILITY};
use idbd_lab::race::{RACE_LMS_STEP_SIZE, RACE_THETA, Race, STILL_RACE_LMS_STEP_SIZE};
use wasm_bindgen::JsError;
use wasm_bindgen::prelude::wasm_bindgen;

use crate::{count_to_f64, js, json_number, seed_from_number};

/// Describes the house and the race parameters as JSON, for the page's copy and labels.
#[must_use]
#[wasm_bindgen(js_name = houseJson)]
pub fn house_json() -> String {
    let names = SENSORS
        .iter()
        .map(|name| format!("\"{name}\""))
        .collect::<Vec<_>>()
        .join(",");
    let levels = LEVELS
        .iter()
        .map(|[strong, weak]| format!("[{},{}]", json_number(*strong), json_number(*weak)))
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "{{\"sensors\":[{names}],\"relevant\":{RELEVANT},\"levels\":[{levels}],\
         \"switchProbability\":{},\"noise\":{},\"initialStepSize\":{},\"theta\":{},\
         \"lmsStepSize\":{},\"stillLmsStepSize\":{}}}",
        json_number(SWITCH_PROBABILITY),
        json_number(NOISE),
        json_number(IDBD_INITIAL_STEP_SIZE),
        json_number(RACE_THETA),
        json_number(RACE_LMS_STEP_SIZE),
        json_number(STILL_RACE_LMS_STEP_SIZE),
    )
}

/// IDBD and a fixed-step LMS learning one seeded house.
#[wasm_bindgen]
#[derive(Debug)]
pub struct HouseRace {
    inner: Race,
}

impl HouseRace {
    fn create(seed: f64) -> Result<Self, String> {
        Ok(Self {
            inner: Race::new(seed_from_number(seed)?),
        })
    }

    fn create_with_lms_step_size(seed: f64, lms_step_size: f64) -> Result<Self, String> {
        Ok(Self {
            inner: Race::with_lms_step_size(seed_from_number(seed)?, lms_step_size)
                .map_err(|error| error.to_string())?,
        })
    }

    fn create_still(seed: f64) -> Result<Self, String> {
        Ok(Self {
            inner: Race::still(seed_from_number(seed)?),
        })
    }
}

#[wasm_bindgen]
impl HouseRace {
    /// Starts a race before the first reading.
    ///
    /// # Errors
    ///
    /// Returns an error unless `seed` is a whole number in `0..2^53`.
    #[wasm_bindgen(constructor)]
    pub fn new(seed: f64) -> Result<Self, JsError> {
        Self::create(seed).map_err(|message| js(&message))
    }

    /// Starts a race on the still house, where nothing switches.
    ///
    /// # Errors
    ///
    /// Returns an error unless `seed` is a whole number in `0..2^53`.
    pub fn still(seed: f64) -> Result<Self, JsError> {
        Self::create_still(seed).map_err(|message| js(&message))
    }

    /// Starts a race on the switching house with a fixed LMS step size of the caller's
    /// choice. The readings and IDBD match `new` for the same seed.
    ///
    /// # Errors
    ///
    /// Returns an error unless `seed` is a whole number in `0..2^53` and `lms_step_size`
    /// is finite and positive.
    #[wasm_bindgen(js_name = withLmsStepSize)]
    pub fn with_lms_step_size(seed: f64, lms_step_size: f64) -> Result<Self, JsError> {
        Self::create_with_lms_step_size(seed, lms_step_size).map_err(|message| js(&message))
    }

    /// Returns the fixed LMS step size this race uses.
    #[must_use]
    #[wasm_bindgen(js_name = lmsStepSize)]
    pub fn lms_step_size(&self) -> f64 {
        self.inner.lms_step_size()
    }

    /// Lets both learners learn from the next reading, or neither.
    ///
    /// # Errors
    ///
    /// Returns the failure message if a learner rejects the reading or the race already
    /// stopped.
    pub fn step(&mut self) -> Result<(), JsError> {
        self.inner.step().map_err(|message| js(&message))
    }

    /// Steps up to `readings` times and returns the number completed.
    ///
    /// # Errors
    ///
    /// Returns the failure message if the first attempted reading fails.
    pub fn run(&mut self, readings: u32) -> Result<u32, JsError> {
        self.inner.run(readings).map_err(|message| js(&message))
    }

    /// Returns the number of readings both learners completed.
    #[must_use]
    pub fn completed(&self) -> f64 {
        count_to_f64(self.inner.completed())
    }

    /// Returns the number of sensors.
    #[must_use]
    #[wasm_bindgen(js_name = numSensors)]
    pub fn num_sensors(&self) -> usize {
        NUM_SENSORS
    }

    /// Returns the sensor values of the last completed reading.
    #[must_use]
    pub fn features(&self) -> Vec<f64> {
        self.inner.features().to_vec()
    }

    /// Returns the temperature change of the last completed reading.
    #[must_use]
    pub fn target(&self) -> f64 {
        self.inner.target()
    }

    /// Returns IDBD's prediction for the last completed reading, made before learning.
    #[must_use]
    #[wasm_bindgen(js_name = idbdPrediction)]
    pub fn idbd_prediction(&self) -> Option<f64> {
        self.inner.idbd_prediction()
    }

    /// Returns LMS's prediction for the last completed reading, made before learning.
    #[must_use]
    #[wasm_bindgen(js_name = lmsPrediction)]
    pub fn lms_prediction(&self) -> Option<f64> {
        self.inner.lms_prediction()
    }

    /// Returns IDBD's step size for every sensor.
    #[must_use]
    #[wasm_bindgen(js_name = idbdStepSizes)]
    pub fn idbd_step_sizes(&self) -> Vec<f64> {
        self.inner.idbd_step_sizes()
    }

    /// Returns IDBD's weights.
    #[must_use]
    #[wasm_bindgen(js_name = idbdWeights)]
    pub fn idbd_weights(&self) -> Vec<f64> {
        self.inner.idbd_weights().to_vec()
    }

    /// Returns LMS's weights.
    #[must_use]
    #[wasm_bindgen(js_name = lmsWeights)]
    pub fn lms_weights(&self) -> Vec<f64> {
        self.inner.lms_weights().to_vec()
    }

    /// Returns the influences that generated the last reading.
    #[must_use]
    pub fn coefficients(&self) -> Vec<f64> {
        self.inner.coefficients().to_vec()
    }

    /// Returns 1 for each relevant sensor whose influence is strong right now, else 0.
    #[must_use]
    pub fn strong(&self) -> Vec<u8> {
        self.inner.strong().iter().map(|&s| u8::from(s)).collect()
    }

    /// Returns the relevant sensors that switched before the last reading.
    #[must_use]
    pub fn switched(&self) -> Vec<u32> {
        self.inner
            .switched()
            .iter()
            .filter_map(|&s| u32::try_from(s).ok())
            .collect()
    }

    /// Returns `true` once a failure stopped the race.
    #[must_use]
    #[wasm_bindgen(js_name = hasFailed)]
    pub fn has_failed(&self) -> bool {
        self.inner.failure().is_some()
    }

    /// Returns the failure message, or `undefined` while the race runs.
    #[must_use]
    #[wasm_bindgen(js_name = failureMessage)]
    pub fn failure_message(&self) -> Option<String> {
        self.inner.failure().map(str::to_owned)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_house_description_parses_and_names_every_sensor() {
        let json = house_json();
        assert!(
            json.starts_with("{\"sensors\":[\"sun on window\",\"oven\""),
            "{json}"
        );
        assert!(json.contains("\"relevant\":3"), "{json}");
        assert!(
            json.contains("\"levels\":[[1.2,0.2],[0.9,0.1],[-1.1,-0.1]]"),
            "{json}"
        );
        assert!(json.contains("\"theta\":0.015"), "{json}");
        assert!(json.contains("\"noise\":0.5"), "{json}");
        assert!(json.contains("\"lmsStepSize\":0.06"), "{json}");
        assert!(json.contains("\"stillLmsStepSize\":0.001"), "{json}");
        assert_eq!(json.matches("\",\"").count() + 1, NUM_SENSORS, "{json}");
    }

    #[test]
    fn a_race_reports_the_lab_race() {
        let mut race = HouseRace::create(4.0).expect("valid seed");
        let mut lab = Race::new(4);
        for _ in 0..300 {
            race.inner.step().expect("finite");
            lab.step().expect("finite");
        }
        assert!((race.completed() - 300.0).abs() < f64::EPSILON);
        assert_eq!(race.num_sensors(), NUM_SENSORS);
        assert_eq!(race.features(), lab.features());
        assert_eq!(race.target().to_bits(), lab.target().to_bits());
        assert_eq!(race.idbd_prediction(), lab.idbd_prediction());
        assert_eq!(race.lms_prediction(), lab.lms_prediction());
        assert_eq!(race.idbd_step_sizes(), lab.idbd_step_sizes());
        assert_eq!(race.idbd_weights(), lab.idbd_weights());
        assert_eq!(race.lms_weights(), lab.lms_weights());
        assert_eq!(race.coefficients(), lab.coefficients());
        assert_eq!(race.strong().len(), RELEVANT);
        assert!(race.switched().iter().all(|&s| s < 3));
        assert!(!race.has_failed());
        assert_eq!(race.failure_message(), None);
    }

    #[test]
    fn a_still_race_reports_its_own_step_size() {
        let race = HouseRace::create_still(1.0).expect("valid seed");
        assert!((race.lms_step_size() - STILL_RACE_LMS_STEP_SIZE).abs() < f64::EPSILON);
    }

    #[test]
    fn a_custom_lms_race_uses_its_step_size() {
        let race = HouseRace::create_with_lms_step_size(1.0, 0.2).expect("valid");
        assert!((race.lms_step_size() - 0.2).abs() < f64::EPSILON);
        assert!(HouseRace::create_with_lms_step_size(1.0, 0.0).is_err());
        assert!(HouseRace::create_with_lms_step_size(-1.0, 0.2).is_err());
    }

    #[test]
    fn an_invalid_seed_is_rejected() {
        assert!(HouseRace::create(-1.0).is_err());
        assert!(HouseRace::create(0.5).is_err());
        assert!(HouseRace::create_still(-1.0).is_err());
    }
}
