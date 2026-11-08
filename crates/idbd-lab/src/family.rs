//! The Family Assistant home behind the story page's game: twelve sensors, four of which
//! add to the home's power draw during their own hours of the day.
//!
//! This is an extension, not a task from Sutton (1992). Each sensor reading is a standard
//! normal scaled by that sensor's own reading scale, so a barometer and an oven
//! thermometer swing by different amounts. The target is the power draw above the
//! always-on base, in kW: the sum of the active sensors' influences times their
//! standardized readings, plus Gaussian noise. One reading arrives per simulated minute.
//!
//! The game draws, for every sensor and hour, the learner's weight averaged over that
//! hour and scaled to kW per standard deviation of the reading. That is what the learner
//! believes the sensor adds at that hour. Step sizes do not localize in time on this task,
//! so they are reported per sensor, not per hour.

use idbd::{Config, Idbd};

use crate::evaluation::IDBD_INITIAL_STEP_SIZE;
use crate::lms::{Lms, LmsError};
use crate::rng::Rng;

/// Hours in a day.
pub const HOURS: usize = 24;

/// Readings per simulated hour, one a minute. At 12 an hour the weights trail the
/// schedule by one to two hours.
pub const READINGS_PER_HOUR: usize = 60;

/// Simulated days per run. The bars pool all of them.
pub const DAYS: usize = 3;

/// Readings in one run.
pub const READINGS: usize = HOURS * READINGS_PER_HOUR * DAYS;

/// Readings before the error starts to count: the first day.
pub const WARMUP_READINGS: usize = HOURS * READINGS_PER_HOUR;

/// Standard deviation of the noise on every target, in kW.
pub const NOISE: f64 = 0.5;

/// Smallest bar peak, in kW per standard deviation, that counts as the sensor mattering
/// at all. Below it the lane reads as "never matters".
pub const FOUND_FLOOR: f64 = 0.25;

/// An hour is found when its bar reaches this fraction of the lane's peak.
pub const FOUND_FRACTION: f64 = 0.5;

/// IDBD's meta step size for this home, selected by scenario E07 on the tuning seeds.
/// `family-eval` fails its run when its selection no longer matches.
pub const FAMILY_THETA: f64 = 0.07;

/// The fixed LMS step size for this home, selected by scenario E07 on the tuning seeds.
pub const FAMILY_LMS_STEP_SIZE: f64 = 0.01;

/// Seed offset for the noise stream, so it never changes the readings.
const NOISE_STREAM: u64 = 0x4641_4D49_4C59;

/// A set of hours of the day, one bit per hour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct HourSet(u32);

impl HourSet {
    /// The empty set: a sensor that never matters.
    pub const EMPTY: Self = Self(0);

    /// The hours from `start` up to but not including `end`, wrapping past midnight when
    /// `end` is not after `start`. Returns `None` unless both are below 24 and differ.
    #[must_use]
    pub const fn range(start: usize, end: usize) -> Option<Self> {
        if start >= HOURS || end >= HOURS || start == end {
            return None;
        }
        let mut bits = 0_u32;
        let mut hour = start;
        while hour != end {
            bits |= 1 << hour;
            hour = (hour + 1) % HOURS;
        }
        Some(Self(bits))
    }

    /// The set with these hour bits, hour 0 in the lowest. Returns `None` if a bit at or
    /// above hour 24 is set.
    #[must_use]
    pub const fn from_bits(bits: u32) -> Option<Self> {
        if bits >> HOURS == 0 {
            Some(Self(bits))
        } else {
            None
        }
    }

    /// Returns `true` if `hour` is in the set.
    #[must_use]
    pub const fn contains(self, hour: usize) -> bool {
        hour < HOURS && self.0 & (1 << hour) != 0
    }

    /// Returns the number of hours in the set.
    #[must_use]
    pub const fn len(self) -> u32 {
        self.0.count_ones()
    }

    /// Returns `true` if the set has no hours.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Returns the set as a bit mask, hour 0 in the lowest bit.
    #[must_use]
    pub const fn bits(self) -> u32 {
        self.0
    }

    /// Jaccard overlap with `other`: shared hours over hours in either. Two empty sets
    /// agree fully and score 1.
    #[must_use]
    pub fn overlap(self, other: Self) -> f64 {
        let union = (self.0 | other.0).count_ones();
        if union == 0 {
            return 1.0;
        }
        f64::from((self.0 & other.0).count_ones()) / f64::from(union)
    }
}

/// One sensor of the home.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sensor {
    /// The name shown on the page.
    pub name: &'static str,
    /// Standard deviation of the raw reading.
    pub scale: f64,
    /// The hours in which the sensor adds to the power draw.
    pub hours: HourSet,
    /// kW added per standard deviation of the reading during those hours.
    pub influence: f64,
}

const fn hours(start: usize, end: usize) -> HourSet {
    match HourSet::range(start, end) {
        Some(set) => set,
        None => panic!("a sensor's hours must be a valid range"),
    }
}

const fn active(
    name: &'static str,
    scale: f64,
    start: usize,
    end: usize,
    influence: f64,
) -> Sensor {
    Sensor {
        name,
        scale,
        hours: hours(start, end),
        influence,
    }
}

const fn idle(name: &'static str, scale: f64) -> Sensor {
    Sensor {
        name,
        scale,
        hours: HourSet::EMPTY,
        influence: 0.0,
    }
}

/// Number of sensors in the home.
pub const NUM_SENSORS: usize = 12;

/// The sensors, in the order the page shows them. The four that matter are spread among
/// the eight that never do, so the order gives nothing away.
pub const SENSORS: [Sensor; NUM_SENSORS] = [
    idle("Wi-Fi signal", 1.0),
    active("Water heater temperature", 0.5, 6, 8, 1.2),
    idle("Doorbell battery", 2.0),
    active("Wall temperature", 1.0, 5, 9, 0.9),
    idle("Barometer", 0.5),
    active("Oven temperature", 2.0, 17, 20, 1.0),
    idle("Humidity", 1.0),
    idle("CO2", 2.0),
    active("Wi-Fi data use", 0.5, 20, 23, 0.6),
    idle("Air quality (PM2.5)", 1.0),
    idle("Front door", 2.0),
    idle("Plant soil moisture", 0.5),
];

/// The hour of the day of reading `index`, counted from midnight of the first day.
#[must_use]
pub const fn hour_of(index: usize) -> usize {
    (index / READINGS_PER_HOUR) % HOURS
}

/// The hours in which a bar marks its sensor as mattering: those at or above
/// [`FOUND_FRACTION`] of the lane's peak, or none when the peak is below [`FOUND_FLOOR`].
#[must_use]
pub fn found_hours(bar: &[f64; HOURS]) -> HourSet {
    let peak = bar.iter().fold(0.0_f64, |max, value| max.max(value.abs()));
    if peak < FOUND_FLOOR {
        return HourSet::EMPTY;
    }
    let bits = bar
        .iter()
        .enumerate()
        .filter(|(_, value)| value.abs() >= FOUND_FRACTION * peak)
        .fold(0_u32, |bits, (hour, _)| bits | 1 << hour);
    HourSet(bits)
}

/// The overlap of `guesses` with the true hours, pooled over every sensor: hours guessed
/// right on any sensor over hours in either the guess or the truth. Leaving every sensor
/// empty scores 0, because four sensors matter, and every wrongly claimed hour lowers it.
///
/// # Panics
///
/// Panics if `guesses` does not hold one set per sensor.
#[must_use]
pub fn score(guesses: &[HourSet]) -> f64 {
    assert_eq!(guesses.len(), NUM_SENSORS, "one guess per sensor");
    let (shared, union) =
        guesses
            .iter()
            .zip(&SENSORS)
            .fold((0_u32, 0_u32), |(shared, union), (guess, sensor)| {
                (
                    shared + (guess.0 & sensor.hours.0).count_ones(),
                    union + (guess.0 | sensor.hours.0).count_ones(),
                )
            });
    f64::from(shared) / f64::from(union)
}

/// A seeded home that produces one reading a minute.
#[derive(Debug, Clone)]
pub struct Home {
    inputs: Rng,
    noise: Rng,
    generated: usize,
}

impl Home {
    /// Creates a home. The same seed always produces the same readings and noise.
    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self {
            inputs: Rng::new(seed),
            noise: Rng::new(seed ^ NOISE_STREAM),
            generated: 0,
        }
    }

    /// Returns the number of readings generated so far.
    #[must_use]
    pub const fn generated(&self) -> usize {
        self.generated
    }

    /// Writes the next reading's raw sensor values into `features` and returns the power
    /// draw above the base, in kW.
    pub fn next_sample(&mut self, features: &mut [f64; NUM_SENSORS]) -> f64 {
        let hour = hour_of(self.generated);
        let mut signal = 0.0;
        for (x, sensor) in features.iter_mut().zip(&SENSORS) {
            let z = self.inputs.standard_normal();
            *x = sensor.scale * z;
            if sensor.hours.contains(hour) {
                signal += sensor.influence * z;
            }
        }
        self.generated += 1;
        signal + NOISE * self.noise.standard_normal()
    }
}

/// Per-sensor, per-hour means of a learner's weights in kW per standard deviation.
#[derive(Debug, Clone)]
pub struct Bars {
    sums: [[f64; HOURS]; NUM_SENSORS],
    counts: [u32; HOURS],
}

impl Default for Bars {
    fn default() -> Self {
        Self::new()
    }
}

impl Bars {
    /// Creates empty bars.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            sums: [[0.0; HOURS]; NUM_SENSORS],
            counts: [0; HOURS],
        }
    }

    /// Adds the weights held after a reading in `hour`, already in kW per standard
    /// deviation of each reading.
    ///
    /// # Panics
    ///
    /// Panics unless `hour` is below 24 and `weights` holds one value per sensor.
    pub fn add(&mut self, hour: usize, weights: &[f64]) {
        assert_eq!(weights.len(), NUM_SENSORS, "one weight per sensor");
        for (sums, w) in self.sums.iter_mut().zip(weights) {
            sums[hour] += w;
        }
        self.counts[hour] += 1;
    }

    /// Returns the bar of `sensor`: the mean per hour, zero for hours with no reading yet.
    ///
    /// # Panics
    ///
    /// Panics if `sensor` is not below [`NUM_SENSORS`].
    #[must_use]
    pub fn bar(&self, sensor: usize) -> [f64; HOURS] {
        let mut bar = [0.0; HOURS];
        for ((value, sum), count) in bar.iter_mut().zip(&self.sums[sensor]).zip(&self.counts) {
            if *count > 0 {
                *value = sum / f64::from(*count);
            }
        }
        bar
    }

    /// Returns the hours each sensor's bar marks as mattering.
    #[must_use]
    pub fn found(&self) -> [HourSet; NUM_SENSORS] {
        std::array::from_fn(|sensor| found_hours(&self.bar(sensor)))
    }

    /// Returns the mean overlap of the found hours with the true hours.
    #[must_use]
    pub fn score(&self) -> f64 {
        score(&self.found())
    }
}

/// LMS step sizes offered to scenario E07's selection.
pub const LMS_GRID: [f64; 11] = [
    0.001, 0.002, 0.005, 0.01, 0.02, 0.03, 0.04, 0.05, 0.06, 0.08, 0.10,
];

/// IDBD meta step sizes offered to scenario E07's selection. It reaches higher than the
/// grid of the other scenarios because this home's best value lies above 0.015.
pub const THETA_GRID: [f64; 11] = [
    0.0, 0.001, 0.002, 0.005, 0.01, 0.015, 0.02, 0.03, 0.05, 0.07, 0.10,
];

/// One learner and its parameter.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Learner {
    /// IDBD with this meta step size, starting at the shared initial step size.
    Idbd {
        /// The meta step size.
        theta: f64,
    },
    /// LMS with this fixed step size.
    Lms {
        /// The step size.
        step_size: f64,
    },
    /// LMS with this fixed step size on readings divided by each sensor's scale, so
    /// every reading has a standard deviation of 1. A control for the scale difference.
    RescaledLms {
        /// The step size.
        step_size: f64,
    },
}

/// Whether a learner sees the raw readings or readings rescaled to unit spread.
enum Readings {
    Raw,
    Rescaled,
}

enum Model {
    Idbd(Idbd),
    Lms(Lms, Readings),
}

impl Model {
    fn new(learner: Learner) -> Result<Self, String> {
        match learner {
            Learner::Idbd { theta } => {
                let config =
                    Config::new(IDBD_INITIAL_STEP_SIZE, theta).map_err(|e| e.to_string())?;
                Ok(Self::Idbd(
                    Idbd::new(NUM_SENSORS, config).map_err(|e| e.to_string())?,
                ))
            }
            Learner::Lms { step_size } => Ok(Self::Lms(
                Lms::new(NUM_SENSORS, step_size).map_err(|e| e.to_string())?,
                Readings::Raw,
            )),
            Learner::RescaledLms { step_size } => Ok(Self::Lms(
                Lms::new(NUM_SENSORS, step_size).map_err(|e| e.to_string())?,
                Readings::Rescaled,
            )),
        }
    }

    fn update(&mut self, features: &[f64], target: f64) -> Result<f64, String> {
        match self {
            Self::Idbd(idbd) => idbd
                .update(features, target)
                .map(|u| u.error)
                .map_err(|e| e.to_string()),
            Self::Lms(lms, Readings::Raw) => lms
                .update(features, target)
                .map(|o| o.error)
                .map_err(|e| e.to_string()),
            Self::Lms(lms, Readings::Rescaled) => {
                let rescaled: [f64; NUM_SENSORS] =
                    std::array::from_fn(|i| features[i] / SENSORS[i].scale);
                lms.update(&rescaled, target)
                    .map(|o| o.error)
                    .map_err(|e| e.to_string())
            }
        }
    }

    /// The weights in kW per standard deviation of each reading.
    fn weights_per_sd(&self) -> [f64; NUM_SENSORS] {
        match self {
            Self::Idbd(idbd) => std::array::from_fn(|i| idbd.weights()[i] * SENSORS[i].scale),
            Self::Lms(lms, Readings::Raw) => {
                std::array::from_fn(|i| lms.weights()[i] * SENSORS[i].scale)
            }
            Self::Lms(lms, Readings::Rescaled) => std::array::from_fn(|i| lms.weights()[i]),
        }
    }

    /// The step sizes in units of a reading with unit spread.
    fn speeds(&self) -> [f64; NUM_SENSORS] {
        let mut speeds = [0.0; NUM_SENSORS];
        for (i, (speed, sensor)) in speeds.iter_mut().zip(&SENSORS).enumerate() {
            *speed = match self {
                Self::Idbd(idbd) => {
                    idbd.step_sizes().nth(i).expect("one step size per sensor")
                        * sensor.scale
                        * sensor.scale
                }
                Self::Lms(lms, Readings::Raw) => lms.step_size() * sensor.scale * sensor.scale,
                Self::Lms(lms, Readings::Rescaled) => lms.step_size(),
            };
        }
        speeds
    }
}

/// What one learner did over a full run.
#[derive(Debug, Clone)]
pub struct Run {
    /// Mean squared error after the first day.
    pub mean_squared_error: f64,
    /// The learner's bars.
    pub bars: Bars,
    /// Learning speed per sensor at the end, in standardized units.
    pub speeds: [f64; NUM_SENSORS],
}

/// Runs one learner over a full run of the home with this seed.
///
/// # Errors
///
/// Returns a message naming the reading if the learner rejects its parameter or a
/// reading.
pub fn run(seed: u64, learner: Learner) -> Result<Run, String> {
    let mut model = Model::new(learner)?;
    let mut home = Home::new(seed);
    let mut bars = Bars::new();
    let mut squared_error = 0.0;
    let mut features = [0.0; NUM_SENSORS];
    for index in 0..READINGS {
        let target = home.next_sample(&mut features);
        let error = model
            .update(&features, target)
            .map_err(|e| format!("reading {}: {e}", index + 1))?;
        if index >= WARMUP_READINGS {
            squared_error += error * error;
        }
        bars.add(hour_of(index), &model.weights_per_sd());
    }
    let measured = f64::from(u32::try_from(READINGS - WARMUP_READINGS).expect("small"));
    Ok(Run {
        mean_squared_error: squared_error / measured,
        bars,
        speeds: model.speeds(),
    })
}

/// One grid value of a selection: its mean error over the seeds that completed, and how
/// many failed.
#[derive(Debug, Clone, PartialEq)]
pub struct GridPoint {
    /// The parameter value.
    pub value: f64,
    /// Mean of the per-seed mean squared errors, over completed seeds.
    pub mean_squared_error: Option<f64>,
    /// Seeds whose run failed.
    pub failures: usize,
}

/// Runs every grid value on every seed and returns the points with the index of the
/// lowest mean error among the values with no failure.
///
/// # Errors
///
/// Returns a message if no grid value completed on every seed.
pub fn select(
    grid: &[f64],
    seeds: &[u64],
    learner: fn(f64) -> Learner,
) -> Result<(Vec<GridPoint>, usize), String> {
    let points: Vec<GridPoint> = grid
        .iter()
        .map(|&value| {
            let errors: Vec<f64> = seeds
                .iter()
                .filter_map(|&seed| run(seed, learner(value)).ok())
                .map(|r| r.mean_squared_error)
                .collect();
            let completed = u32::try_from(errors.len()).expect("few seeds");
            GridPoint {
                value,
                mean_squared_error: (completed > 0)
                    .then(|| errors.iter().sum::<f64>() / f64::from(completed)),
                failures: seeds.len() - errors.len(),
            }
        })
        .collect();
    let best = points
        .iter()
        .enumerate()
        .filter(|(_, p)| p.failures == 0)
        .filter_map(|(i, p)| p.mean_squared_error.map(|e| (i, e)))
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(i, _)| i)
        .ok_or("no grid value completed on every seed")?;
    Ok((points, best))
}

/// IDBD and a fixed-step LMS learning one seeded home, one reading at a time.
#[derive(Debug, Clone)]
pub struct Day {
    home: Home,
    idbd: Idbd,
    lms: Lms,
    idbd_bars: Bars,
    lms_bars: Bars,
    idbd_squared_error: f64,
    lms_squared_error: f64,
    completed: usize,
    failure: Option<String>,
}

impl Day {
    /// Starts a run with the parameters E07 selected for this home.
    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self::with_parameters(seed, FAMILY_THETA, FAMILY_LMS_STEP_SIZE)
            .expect("the selected parameters are valid")
    }

    /// Starts a run with IDBD's meta step size `theta` and the fixed step `lms_step_size`.
    ///
    /// # Errors
    ///
    /// Returns a message if either parameter is rejected by its learner.
    pub fn with_parameters(seed: u64, theta: f64, lms_step_size: f64) -> Result<Self, String> {
        let config =
            Config::new(IDBD_INITIAL_STEP_SIZE, theta).map_err(|e| format!("IDBD: {e}"))?;
        Ok(Self {
            home: Home::new(seed),
            idbd: Idbd::new(NUM_SENSORS, config).map_err(|e| format!("IDBD: {e}"))?,
            lms: Lms::new(NUM_SENSORS, lms_step_size).map_err(|e: LmsError| format!("LMS: {e}"))?,
            idbd_bars: Bars::new(),
            lms_bars: Bars::new(),
            idbd_squared_error: 0.0,
            lms_squared_error: 0.0,
            completed: 0,
            failure: None,
        })
    }

    /// Lets both learners learn from the next reading, or neither.
    ///
    /// # Errors
    ///
    /// Returns the failure message if either learner rejects the reading, if an earlier
    /// failure stopped the run, or if the run is complete. Neither learner changes then.
    pub fn step(&mut self) -> Result<(), String> {
        if let Some(failure) = &self.failure {
            return Err(failure.clone());
        }
        let index = self.completed;
        if index >= READINGS {
            return Err(format!("the run ends after {READINGS} readings"));
        }
        let mut features = [0.0; NUM_SENSORS];
        let target = self.home.next_sample(&mut features);
        let checked = self
            .lms
            .check(&features, target)
            .map_err(|e| format!("LMS: {e}"))
            .and_then(|_| {
                self.idbd
                    .update(&features, target)
                    .map_err(|e| format!("IDBD: {e}"))
            });
        let update = match checked {
            Ok(update) => update,
            Err(message) => {
                let failure = format!("reading {}: {message}", index + 1);
                self.failure = Some(failure.clone());
                return Err(failure);
            }
        };
        let outcome = self
            .lms
            .update(&features, target)
            .expect("a successful check guarantees the update");
        if index >= WARMUP_READINGS {
            self.idbd_squared_error += update.error * update.error;
            self.lms_squared_error += outcome.error * outcome.error;
        }
        let hour = hour_of(index);
        self.idbd_bars.add(hour, &self.idbd_weights());
        self.lms_bars.add(hour, &self.lms_weights());
        self.completed += 1;
        Ok(())
    }

    /// Steps up to `readings` times, stopping at the end of the run.
    ///
    /// # Errors
    ///
    /// Returns the failure message if a reading fails.
    pub fn run(&mut self, readings: usize) -> Result<(), String> {
        let end = READINGS.min(self.completed.saturating_add(readings));
        while self.completed() < end {
            self.step()?;
        }
        Ok(())
    }

    /// Runs to the end of the last day.
    ///
    /// # Errors
    ///
    /// Returns the failure message if a reading fails.
    pub fn finish(&mut self) -> Result<(), String> {
        self.run(READINGS)
    }

    /// Returns the number of readings both learners completed.
    #[must_use]
    pub const fn completed(&self) -> usize {
        self.completed
    }

    /// Returns the failure that stopped the run, if any.
    #[must_use]
    pub fn failure(&self) -> Option<&str> {
        self.failure.as_deref()
    }

    /// Returns IDBD's bars.
    #[must_use]
    pub const fn idbd_bars(&self) -> &Bars {
        &self.idbd_bars
    }

    /// Returns the fixed-step learner's bars.
    #[must_use]
    pub const fn lms_bars(&self) -> &Bars {
        &self.lms_bars
    }

    /// Returns IDBD's current weights in kW per standard deviation of each reading.
    #[must_use]
    pub fn idbd_weights(&self) -> [f64; NUM_SENSORS] {
        std::array::from_fn(|i| self.idbd.weights()[i] * SENSORS[i].scale)
    }

    /// Returns the fixed-step learner's current weights in kW per standard deviation.
    #[must_use]
    pub fn lms_weights(&self) -> [f64; NUM_SENSORS] {
        std::array::from_fn(|i| self.lms.weights()[i] * SENSORS[i].scale)
    }

    /// Returns IDBD's learning speed per sensor in the units of a standardized reading,
    /// `alpha_i * scale_i^2`, so sensors with different reading scales compare directly.
    #[must_use]
    pub fn idbd_speeds(&self) -> [f64; NUM_SENSORS] {
        let mut speeds = [0.0; NUM_SENSORS];
        for ((speed, alpha), sensor) in speeds.iter_mut().zip(self.idbd.step_sizes()).zip(&SENSORS)
        {
            *speed = alpha * sensor.scale * sensor.scale;
        }
        speeds
    }

    /// Returns the fixed-step learner's speed per sensor in the same units.
    #[must_use]
    pub fn lms_speeds(&self) -> [f64; NUM_SENSORS] {
        std::array::from_fn(|i| self.lms.step_size() * SENSORS[i].scale * SENSORS[i].scale)
    }

    /// Returns IDBD's and the fixed-step learner's mean squared error after the first
    /// day, or `None` before a measured reading.
    #[must_use]
    pub fn mean_squared_errors(&self) -> Option<(f64, f64)> {
        let measured = self.completed().checked_sub(WARMUP_READINGS)?;
        if measured == 0 {
            return None;
        }
        let n = f64::from(u32::try_from(measured).expect("a run has fewer than 2^32 readings"));
        Some((self.idbd_squared_error / n, self.lms_squared_error / n))
    }
}

#[cfg(test)]
#[expect(clippy::float_cmp, reason = "tests assert exact, deterministic values")]
mod tests {
    use super::*;

    #[test]
    fn a_range_wraps_past_midnight_and_rejects_empty_or_out_of_range_hours() {
        let evening = HourSet::range(20, 23).expect("valid");
        assert_eq!(evening.len(), 3);
        assert!(evening.contains(22) && !evening.contains(23) && !evening.contains(19));
        let night = HourSet::range(22, 2).expect("valid");
        assert_eq!(night.len(), 4);
        assert!(night.contains(23) && night.contains(0) && night.contains(1) && !night.contains(2));
        assert_eq!(HourSet::range(5, 5), None);
        assert_eq!(HourSet::range(24, 2), None);
        assert_eq!(HourSet::range(2, 24), None);
        assert!(!evening.contains(24));
    }

    #[test]
    fn sensor_hours_build_a_range_and_reject_an_empty_one() {
        assert_eq!(hours(6, 8), HourSet::range(6, 8).expect("valid"));
        let empty = std::panic::catch_unwind(|| hours(5, 5));
        assert!(empty.is_err(), "an empty range is a programming error");
    }

    #[test]
    fn overlap_is_shared_hours_over_hours_in_either() {
        let a = HourSet::range(6, 10).expect("valid");
        let b = HourSet::range(8, 12).expect("valid");
        assert_eq!(a.overlap(b), 2.0 / 6.0);
        assert_eq!(a.overlap(a), 1.0);
        assert_eq!(HourSet::EMPTY.overlap(HourSet::EMPTY), 1.0);
        assert_eq!(a.overlap(HourSet::EMPTY), 0.0);
        assert!(HourSet::EMPTY.is_empty() && !a.is_empty());
        assert_eq!(a.bits(), 0b11_1100_0000);
        assert_eq!(HourSet::from_bits(a.bits()), Some(a));
        assert_eq!(HourSet::from_bits(1 << 23).map(HourSet::len), Some(1));
        assert_eq!(HourSet::from_bits(1 << 24), None);
    }

    #[test]
    fn four_sensors_matter_and_the_true_guesses_score_one() {
        let truth: Vec<HourSet> = SENSORS.iter().map(|s| s.hours).collect();
        assert_eq!(truth.iter().filter(|h| !h.is_empty()).count(), 4);
        assert_eq!(score(&truth), 1.0);
        let blank = [HourSet::EMPTY; NUM_SENSORS];
        assert_eq!(score(&blank), 0.0);
        let mut oven_only = blank;
        oven_only[5] = SENSORS[5].hours;
        assert_eq!(score(&oven_only), 3.0 / 12.0);
        let mut extra = truth;
        extra[0] = HourSet::range(0, 4).expect("valid");
        assert_eq!(score(&extra), 12.0 / 16.0);
        for sensor in &SENSORS {
            assert_eq!(
                sensor.hours.is_empty(),
                sensor.influence == 0.0,
                "{}",
                sensor.name
            );
        }
    }

    #[test]
    fn found_hours_take_half_the_peak_above_the_floor() {
        let mut bar = [0.0; HOURS];
        bar[17] = 0.8;
        bar[18] = -1.0;
        bar[19] = 0.5;
        bar[20] = 0.49;
        let found = found_hours(&bar);
        assert_eq!(found, HourSet::range(17, 20).expect("valid"));
        let quiet = [FOUND_FLOOR - 0.01; HOURS];
        assert_eq!(found_hours(&quiet), HourSet::EMPTY);
        let level = [FOUND_FLOOR; HOURS];
        assert_eq!(found_hours(&level).len(), 24);
    }

    #[test]
    fn readings_follow_the_hour_and_the_documented_noise() {
        assert_eq!(hour_of(0), 0);
        assert_eq!(hour_of(59), 0);
        assert_eq!(hour_of(60), 1);
        assert_eq!(hour_of(WARMUP_READINGS), 0);
        let mut home = Home::new(4);
        let mut x = [0.0; NUM_SENSORS];
        let mut residuals = Vec::new();
        for _ in 0..WARMUP_READINGS {
            let hour = hour_of(home.generated());
            let target = home.next_sample(&mut x);
            let signal: f64 = SENSORS
                .iter()
                .zip(&x)
                .filter(|(s, _)| s.hours.contains(hour))
                .map(|(s, v)| s.influence * v / s.scale)
                .sum();
            residuals.push(target - signal);
        }
        let n = f64::from(u32::try_from(residuals.len()).expect("small"));
        let mean = residuals.iter().sum::<f64>() / n;
        let sd = (residuals.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / n).sqrt();
        assert!(mean.abs() < 0.04, "noise mean {mean}");
        assert!((sd - NOISE).abs() < 0.03, "noise sd {sd} vs {NOISE}");
        assert_eq!(home.generated(), WARMUP_READINGS);
    }

    const RECORDED_SEED_7: [u64; 3] = [
        4_588_213_298_595_010_603,
        13_818_207_806_458_805_176,
        4_604_595_358_437_932_362,
    ];

    #[test]
    fn seed_7_reproduces_its_recorded_targets() {
        // Recorded from this implementation. A change here changes every published family
        // result, so it must be deliberate. Box-Muller goes through ln and cos, whose last
        // bit differs between platform math libraries, so targets compare to 1e-12.
        let mut home = Home::new(7);
        let mut x = [0.0; NUM_SENSORS];
        let targets: Vec<f64> = (0..400).map(|_| home.next_sample(&mut x)).collect();
        let recorded = [targets[0], targets[361], targets[399]];
        let want = RECORDED_SEED_7.map(f64::from_bits);
        for (got, want) in recorded.iter().zip(want) {
            assert!(
                (got - want).abs() <= 1e-12 * want.abs().max(1.0),
                "{got} vs {want}"
            );
        }
    }

    #[test]
    fn a_huge_batch_finishes_the_run_instead_of_overflowing() {
        let mut day = Day::new(1);
        day.run(1).expect("runs");
        day.run(usize::MAX).expect("runs to the end");
        assert_eq!(day.completed(), READINGS);
    }

    #[test]
    fn the_same_seed_gives_the_same_run() {
        let (mut a, mut b) = (Day::new(9), Day::new(9));
        a.run(500).expect("runs");
        b.run(500).expect("runs");
        assert_eq!(a.idbd_weights(), b.idbd_weights());
        assert_eq!(a.lms_weights(), b.lms_weights());
    }

    #[test]
    fn a_full_run_finds_the_scheduled_hours_and_then_stops() {
        let mut day = Day::new(1);
        assert_eq!(day.mean_squared_errors(), None);
        day.finish().expect("the selected parameters do not fail");
        assert_eq!(day.completed(), READINGS);
        assert!(day.step().is_err(), "a finished run takes no more readings");
        assert!(day.failure().is_none());
        assert!(day.idbd_bars().score() > 0.8, "{}", day.idbd_bars().score());
        let (idbd, lms) = day.mean_squared_errors().expect("measured");
        assert!(idbd < lms, "IDBD {idbd} vs LMS {lms}");
        assert!(idbd > NOISE * NOISE, "nobody beats the noise: {idbd}");
        let speeds = day.idbd_speeds();
        let quiet = speeds[0].max(speeds[2]);
        assert!(speeds[5] > 2.0 * quiet, "{speeds:?}");
        assert_eq!(day.lms_speeds()[0], FAMILY_LMS_STEP_SIZE);
    }

    #[test]
    fn a_rejected_reading_stops_the_run_and_changes_neither_learner() {
        let mut day = Day::with_parameters(3, FAMILY_THETA, 5.0).expect("valid");
        let snapshot = |day: &Day| {
            (
                day.idbd_weights(),
                day.lms_weights(),
                day.completed(),
                day.mean_squared_errors(),
                day.idbd_bars().bar(5),
            )
        };
        let error = loop {
            let before = snapshot(&day);
            match day.step() {
                Ok(()) => assert!(day.completed() < READINGS, "a step size of 5 diverges"),
                Err(error) => {
                    assert_eq!(
                        snapshot(&day),
                        before,
                        "the rejected reading changed the run"
                    );
                    break error;
                }
            }
        };
        assert_eq!(
            error,
            format!(
                "reading {}: {}",
                day.completed() + 1,
                error.split_once(": ").expect("prefixed").1
            )
        );
        assert_eq!(day.failure(), Some(error.as_str()));
        let weights = (day.idbd_weights(), day.lms_weights(), day.completed());
        assert_eq!(day.step(), Err(error));
        assert_eq!(
            (day.idbd_weights(), day.lms_weights(), day.completed()),
            weights
        );
        assert!(Day::with_parameters(3, -1.0, 0.02).is_err());
        assert!(Day::with_parameters(3, 0.01, 0.0).is_err());
    }

    #[test]
    fn run_matches_the_paired_day_for_each_learner() {
        let mut day = Day::new(2);
        day.finish().expect("completes");
        let idbd = run(
            2,
            Learner::Idbd {
                theta: FAMILY_THETA,
            },
        )
        .expect("completes");
        let lms = run(
            2,
            Learner::Lms {
                step_size: FAMILY_LMS_STEP_SIZE,
            },
        )
        .expect("completes");
        let (idbd_mse, lms_mse) = day.mean_squared_errors().expect("measured");
        assert_eq!(idbd.mean_squared_error.to_bits(), idbd_mse.to_bits());
        assert_eq!(lms.mean_squared_error.to_bits(), lms_mse.to_bits());
        assert_eq!(idbd.bars.bar(5), day.idbd_bars().bar(5));
        assert_eq!(lms.bars.bar(1), day.lms_bars().bar(1));
        assert_eq!(idbd.speeds, day.idbd_speeds());
        assert_eq!(lms.speeds, day.lms_speeds());
    }

    #[test]
    fn rescaled_lms_sees_unit_readings_and_reports_per_sd_values() {
        let raw = run(4, Learner::Lms { step_size: 0.02 }).expect("completes");
        let rescaled = run(4, Learner::RescaledLms { step_size: 0.02 }).expect("completes");
        assert_eq!(rescaled.speeds, [0.02; NUM_SENSORS]);
        assert_eq!(raw.speeds[2], 0.02 * 4.0);
        assert_ne!(
            raw.mean_squared_error.to_bits(),
            rescaled.mean_squared_error.to_bits()
        );
        assert!(
            rescaled.bars.score() > raw.bars.score(),
            "{} vs {}",
            rescaled.bars.score(),
            raw.bars.score()
        );
        assert!(run(4, Learner::RescaledLms { step_size: 0.0 }).is_err());
    }

    #[test]
    fn select_skips_failing_values_and_picks_the_lowest_error() {
        let lms = |step_size| Learner::Lms { step_size };
        let (points, best) = select(&[0.001, 0.02, 5.0], &[1, 2], lms).expect("one completes");
        assert_eq!(points[2].failures, 2);
        assert_eq!(points[2].mean_squared_error, None);
        assert_eq!(best, 1);
        assert!(points[1].mean_squared_error < points[0].mean_squared_error);
        let one = run(1, lms(0.02)).expect("completes").mean_squared_error;
        let two = run(2, lms(0.02)).expect("completes").mean_squared_error;
        assert_eq!(points[1].mean_squared_error, Some(f64::midpoint(one, two)));
        assert!(select(&[5.0], &[1], lms).is_err());
        assert!(run(1, Learner::Idbd { theta: -1.0 }).is_err());
    }

    #[test]
    fn empty_bars_read_zero_and_found_nothing() {
        let bars = Bars::default();
        assert_eq!(bars.bar(0), [0.0; HOURS]);
        assert_eq!(bars.found(), [HourSet::EMPTY; NUM_SENSORS]);
        assert_eq!(bars.score(), 0.0);
        let mut bars = Bars::new();
        let mut weights = [0.0; NUM_SENSORS];
        weights[5] = 0.5;
        bars.add(18, &weights);
        bars.add(18, &[0.0; NUM_SENSORS]);
        assert_eq!(bars.bar(5)[18], 0.25);
    }
}
