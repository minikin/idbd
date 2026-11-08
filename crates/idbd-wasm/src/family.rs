//! The story page's Family Assistant game: IDBD and a tuned fixed-step LMS learning the
//! home of `idbd_lab::family` over three simulated days.
//!
//! Every learning value, bar and score comes from `idbd_lab::family`, which drives the
//! `idbd` core. The page only draws them and collects the visitor's guessed hours.

use idbd_lab::evaluation::{EVALUATION_SEEDS, IDBD_INITIAL_STEP_SIZE};
use idbd_lab::family::{
    DAYS, Day, FAMILY_LMS_STEP_SIZE, FAMILY_THETA, FOUND_FLOOR, FOUND_FRACTION, HOURS, HourSet,
    NOISE, NUM_SENSORS, READINGS, READINGS_PER_HOUR, SENSORS, WARMUP_READINGS, score,
};
use wasm_bindgen::JsError;
use wasm_bindgen::prelude::wasm_bindgen;

use crate::{count_to_f64, js, json_number, seed_from_number};

fn json_string(text: &str) -> String {
    format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
}

/// Describes the home, the game's constants and the evaluated days as JSON.
#[must_use]
#[wasm_bindgen(js_name = familyJson)]
pub fn family_json() -> String {
    let sensors = SENSORS
        .iter()
        .map(|sensor| {
            format!(
                "{{\"name\":{},\"scale\":{},\"hours\":{},\"influence\":{}}}",
                json_string(sensor.name),
                json_number(sensor.scale),
                sensor.hours.bits(),
                json_number(sensor.influence)
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let seeds = EVALUATION_SEEDS.map(|s| s.to_string()).join(",");
    format!(
        "{{\"sensors\":[{sensors}],\"hours\":{HOURS},\"readingsPerHour\":{READINGS_PER_HOUR},\
         \"days\":{DAYS},\"readings\":{READINGS},\"warmupReadings\":{WARMUP_READINGS},\
         \"noise\":{},\"foundFloor\":{},\"foundFraction\":{},\"initialStepSize\":{},\
         \"theta\":{},\"lmsStepSize\":{},\"seeds\":[{seeds}]}}",
        json_number(NOISE),
        json_number(FOUND_FLOOR),
        json_number(FOUND_FRACTION),
        json_number(IDBD_INITIAL_STEP_SIZE),
        json_number(FAMILY_THETA),
        json_number(FAMILY_LMS_STEP_SIZE),
    )
}

fn guesses(bits: &[u32]) -> Result<Vec<HourSet>, String> {
    if bits.len() != NUM_SENSORS {
        return Err(format!(
            "expected {NUM_SENSORS} guesses, got {}",
            bits.len()
        ));
    }
    bits.iter()
        .map(|&b| HourSet::from_bits(b).ok_or_else(|| format!("{b} is not a set of hours")))
        .collect()
}

/// Returns the overlap of each guessed set of hours with the sensor's true hours.
///
/// # Errors
///
/// Returns an error unless `bits` holds one 24-bit hour mask per sensor.
#[wasm_bindgen(js_name = familyOverlaps)]
pub fn family_overlaps(bits: &[u32]) -> Result<Vec<f64>, JsError> {
    let sets = guesses(bits).map_err(|message| js(&message))?;
    Ok(sets
        .iter()
        .zip(&SENSORS)
        .map(|(guess, sensor)| guess.overlap(sensor.hours))
        .collect())
}

/// Returns the game score of the guessed hours: their overlap with the true hours,
/// pooled over every sensor.
///
/// # Errors
///
/// Returns an error unless `bits` holds one 24-bit hour mask per sensor.
#[wasm_bindgen(js_name = familyScore)]
pub fn family_score(bits: &[u32]) -> Result<f64, JsError> {
    guesses(bits)
        .map(|sets| score(&sets))
        .map_err(|message| js(&message))
}

/// IDBD and a fixed-step LMS learning one seeded home over three days.
#[wasm_bindgen]
#[derive(Debug)]
pub struct FamilyDay {
    inner: Day,
}

impl FamilyDay {
    fn create(seed: f64) -> Result<Self, String> {
        Ok(Self {
            inner: Day::new(seed_from_number(seed)?),
        })
    }
}

fn flatten(bar: impl Fn(usize) -> [f64; HOURS]) -> Vec<f64> {
    (0..NUM_SENSORS).flat_map(bar).collect()
}

#[wasm_bindgen]
impl FamilyDay {
    /// Starts a run before the first reading, with the parameters E07 selected.
    ///
    /// # Errors
    ///
    /// Returns an error unless `seed` is a whole number in `0..2^53`.
    #[wasm_bindgen(constructor)]
    pub fn new(seed: f64) -> Result<Self, JsError> {
        Self::create(seed).map_err(|message| js(&message))
    }

    /// Steps up to `readings` times, stopping at the end of the last day.
    ///
    /// # Errors
    ///
    /// Returns the failure message if a learner rejects a reading.
    pub fn run(&mut self, readings: u32) -> Result<(), JsError> {
        let readings = usize::try_from(readings).map_err(|e| js(&e.to_string()))?;
        self.inner.run(readings).map_err(|message| js(&message))
    }

    /// Returns the number of readings both learners completed.
    #[must_use]
    pub fn completed(&self) -> f64 {
        count_to_f64(u64::try_from(self.inner.completed()).unwrap_or(u64::MAX))
    }

    /// Returns the failure that stopped the run, if any.
    #[wasm_bindgen(js_name = failureMessage)]
    #[must_use]
    pub fn failure_message(&self) -> Option<String> {
        self.inner.failure().map(str::to_owned)
    }

    /// Returns IDBD's bars, 24 hours per sensor in sensor order, in kW per standard
    /// deviation of the reading.
    #[wasm_bindgen(js_name = idbdBars)]
    #[must_use]
    pub fn idbd_bars(&self) -> Vec<f64> {
        flatten(|i| self.inner.idbd_bars().bar(i))
    }

    /// Returns the fixed-step learner's bars in the same layout.
    #[wasm_bindgen(js_name = lmsBars)]
    #[must_use]
    pub fn lms_bars(&self) -> Vec<f64> {
        flatten(|i| self.inner.lms_bars().bar(i))
    }

    /// Returns the hours IDBD's bars mark, one 24-bit mask per sensor.
    #[wasm_bindgen(js_name = idbdFound)]
    #[must_use]
    pub fn idbd_found(&self) -> Vec<u32> {
        self.inner.idbd_bars().found().map(HourSet::bits).to_vec()
    }

    /// Returns the hours the fixed-step learner's bars mark, one mask per sensor.
    #[wasm_bindgen(js_name = lmsFound)]
    #[must_use]
    pub fn lms_found(&self) -> Vec<u32> {
        self.inner.lms_bars().found().map(HourSet::bits).to_vec()
    }

    /// Returns IDBD's current weights in kW per standard deviation of each reading.
    #[wasm_bindgen(js_name = idbdWeights)]
    #[must_use]
    pub fn idbd_weights(&self) -> Vec<f64> {
        self.inner.idbd_weights().to_vec()
    }

    /// Returns the fixed-step learner's current weights in the same units.
    #[wasm_bindgen(js_name = lmsWeights)]
    #[must_use]
    pub fn lms_weights(&self) -> Vec<f64> {
        self.inner.lms_weights().to_vec()
    }

    /// Returns IDBD's learning speed per sensor, `alpha_i * scale_i^2`.
    #[wasm_bindgen(js_name = idbdSpeeds)]
    #[must_use]
    pub fn idbd_speeds(&self) -> Vec<f64> {
        self.inner.idbd_speeds().to_vec()
    }

    /// Returns the fixed-step learner's speed per sensor in the same units.
    #[wasm_bindgen(js_name = lmsSpeeds)]
    #[must_use]
    pub fn lms_speeds(&self) -> Vec<f64> {
        self.inner.lms_speeds().to_vec()
    }

    /// Returns IDBD's and the fixed-step learner's mean squared error from the second day
    /// on, or an empty array before the first measured reading.
    #[wasm_bindgen(js_name = meanSquaredErrors)]
    #[must_use]
    pub fn mean_squared_errors(&self) -> Vec<f64> {
        self.inner
            .mean_squared_errors()
            .map_or_else(Vec::new, |(idbd, lms)| vec![idbd, lms])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_family_description_names_every_sensor_and_the_selected_parameters() {
        let json = family_json();
        assert!(
            json.starts_with("{\"sensors\":[{\"name\":\"Wi-Fi signal\",\"scale\":1.0,\"hours\":0"),
            "{json}"
        );
        assert!(
            json.contains("\"name\":\"Oven temperature\",\"scale\":2.0,\"hours\":917504"),
            "{json}"
        );
        assert!(json.contains("\"theta\":0.07"), "{json}");
        assert!(json.contains("\"lmsStepSize\":0.01"), "{json}");
        assert!(json.contains("\"seeds\":[1,2,3,4,5,6,7,8,9,10]"), "{json}");
        assert_eq!(json.matches("\"name\":").count(), NUM_SENSORS, "{json}");
        assert_eq!(json_string("a \"b\" \\"), "\"a \\\"b\\\" \\\\\"");
    }

    #[test]
    fn scores_come_from_the_lab_and_reject_bad_masks() {
        let truth: Vec<u32> = SENSORS.iter().map(|s| s.hours.bits()).collect();
        assert_eq!(guesses(&truth).map(|g| score(&g)), Ok(1.0));
        let blank = vec![0; NUM_SENSORS];
        let sets = guesses(&blank).expect("valid");
        assert!(score(&sets).abs() < f64::EPSILON);
        assert!(
            guesses(&[0; 3])
                .expect_err("short")
                .starts_with("expected 12 guesses")
        );
        let mut bad = blank;
        bad[0] = 1 << 24;
        assert!(
            guesses(&bad)
                .expect_err("bit 24")
                .contains("is not a set of hours")
        );
    }

    #[test]
    fn the_exported_scores_match_the_lab_on_valid_masks() {
        let truth: Vec<u32> = SENSORS.iter().map(|s| s.hours.bits()).collect();
        let mut half = truth;
        half[5] = 0;
        let score = family_score(&half)
            .map_err(|_| "valid masks")
            .expect("valid");
        let sets = guesses(&half).expect("valid");
        assert_eq!(score.to_bits(), idbd_lab::family::score(&sets).to_bits());
        let overlaps = family_overlaps(&half)
            .map_err(|_| "valid masks")
            .expect("valid");
        assert_eq!(overlaps.len(), NUM_SENSORS);
        assert_eq!(overlaps[5].to_bits(), 0.0_f64.to_bits());
        assert_eq!(overlaps[1].to_bits(), 1.0_f64.to_bits());
    }

    #[test]
    fn a_failed_day_reports_its_message_and_run_advances() {
        let mut day = FamilyDay::create(3.0).expect("valid seed");
        day.run(250).map_err(|_| "runs").expect("runs");
        assert_eq!(day.inner.completed(), 250);
        let mut failing = FamilyDay {
            inner: Day::with_parameters(3, FAMILY_THETA, 5.0).expect("valid"),
        };
        let error = failing.inner.finish().expect_err("diverges");
        assert_eq!(failing.failure_message(), Some(error));
    }

    #[test]
    fn a_day_reports_the_lab_day() {
        let mut day = FamilyDay::create(3.0).expect("valid seed");
        let mut lab = Day::new(3);
        day.inner.run(1_000).expect("runs");
        lab.run(1_000).expect("runs");
        assert!((day.completed() - 1_000.0).abs() < f64::EPSILON);
        assert_eq!(day.idbd_weights(), lab.idbd_weights().to_vec());
        assert_eq!(day.lms_weights(), lab.lms_weights().to_vec());
        assert_eq!(day.idbd_speeds(), lab.idbd_speeds().to_vec());
        assert_eq!(day.lms_speeds(), lab.lms_speeds().to_vec());
        assert_eq!(day.idbd_bars().len(), NUM_SENSORS * HOURS);
        assert_eq!(
            day.idbd_bars()[5 * HOURS..6 * HOURS],
            lab.idbd_bars().bar(5)
        );
        assert_eq!(day.lms_bars()[HOURS..2 * HOURS], lab.lms_bars().bar(1));
        assert_eq!(
            day.idbd_found(),
            lab.idbd_bars().found().map(HourSet::bits).to_vec()
        );
        assert_eq!(
            day.lms_found(),
            lab.lms_bars().found().map(HourSet::bits).to_vec()
        );
        assert_eq!(day.mean_squared_errors(), Vec::<f64>::new());
        assert!(day.failure_message().is_none());
        day.inner.finish().expect("completes");
        assert_eq!(day.mean_squared_errors().len(), 2);
        assert!(FamilyDay::create(-1.0).is_err());
    }
}
