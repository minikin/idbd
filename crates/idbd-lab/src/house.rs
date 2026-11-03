//! The thermostat house behind the story page: six sensors, three of which move the
//! room temperature by amounts that keep switching.
//!
//! This is an extension, not a task from Sutton (1992). Inputs are independent standard
//! normals, read as standardized sensor readings. The target is the change in room
//! temperature: the dot product of the readings with the current influences, plus
//! Gaussian measurement noise.

use crate::rng::Rng;

/// Number of sensors in the house.
pub const NUM_SENSORS: usize = 6;

/// Sensor names, in input order. The first [`RELEVANT`] move the temperature.
pub const SENSORS: [&str; NUM_SENSORS] = [
    "sun on window",
    "oven",
    "front door",
    "wi-fi signal",
    "doorbell battery",
    "water heater",
];

/// Number of sensors that move the temperature.
pub const RELEVANT: usize = 3;

/// Strong and weak influence of each relevant sensor, in that order.
pub const LEVELS: [[f64; 2]; RELEVANT] = [[1.2, 0.2], [0.9, 0.1], [-1.1, -0.1]];

/// Probability that a relevant sensor switches between its strong and weak influence
/// before a reading. Every sensor switches independently.
pub const SWITCH_PROBABILITY: f64 = 1.0 / 60.0;

/// Standard deviation of the measurement noise on every target.
pub const NOISE: f64 = 0.5;

/// Seed offsets for the switching and noise streams, so neither changes the readings.
const SWITCH_STREAM: u64 = 0x484F_5553_4501;
const NOISE_STREAM: u64 = 0x484F_5553_4502;

/// A seeded house that produces one reading at a time.
#[derive(Debug, Clone)]
pub struct House {
    seed: u64,
    switch_probability: f64,
    inputs: Rng,
    switches: Rng,
    noise: Rng,
    strong: [bool; RELEVANT],
    generated: u64,
}

impl House {
    /// Creates a house with every relevant sensor at its strong influence. The same
    /// seed always produces the same readings, switches and noise.
    pub fn new(seed: u64) -> Self {
        Self::with_switch_probability(seed, SWITCH_PROBABILITY)
    }

    /// Creates the same house with switching turned off: every relevant sensor keeps its
    /// strong influence. Readings and noise match [`House::new`] for the same seed.
    pub fn still(seed: u64) -> Self {
        Self::with_switch_probability(seed, 0.0)
    }

    fn with_switch_probability(seed: u64, switch_probability: f64) -> Self {
        Self {
            seed,
            switch_probability,
            inputs: Rng::new(seed),
            switches: Rng::new(seed ^ SWITCH_STREAM),
            noise: Rng::new(seed ^ NOISE_STREAM),
            strong: [true; RELEVANT],
            generated: 0,
        }
    }

    /// Returns the seed.
    pub fn seed(&self) -> u64 {
        self.seed
    }

    /// Returns the number of readings generated so far.
    pub fn generated(&self) -> u64 {
        self.generated
    }

    /// Returns `true` if relevant sensor `sensor` has its strong influence.
    ///
    /// # Panics
    ///
    /// Panics if `sensor` is not below [`RELEVANT`].
    pub fn is_strong(&self, sensor: usize) -> bool {
        self.strong[sensor]
    }

    /// Returns the influences that generated the most recent reading.
    pub fn coefficients(&self) -> [f64; NUM_SENSORS] {
        let mut coefficients = [0.0; NUM_SENSORS];
        for (sensor, levels) in LEVELS.iter().enumerate() {
            coefficients[sensor] = levels[usize::from(!self.strong[sensor])];
        }
        coefficients
    }

    /// Applies the switches due before the next reading, then writes its sensor values
    /// into `features` and returns the temperature change.
    ///
    /// `switched` is cleared and receives the relevant sensors that switched before this
    /// reading.
    ///
    /// # Panics
    ///
    /// Panics if `features` does not hold [`NUM_SENSORS`] values.
    pub fn next_sample(&mut self, features: &mut [f64], switched: &mut Vec<usize>) -> f64 {
        assert_eq!(features.len(), NUM_SENSORS, "feature buffer length");
        switched.clear();
        if self.generated > 0 {
            for sensor in 0..RELEVANT {
                if self.switches.uniform() < self.switch_probability {
                    self.strong[sensor] = !self.strong[sensor];
                    switched.push(sensor);
                }
            }
        }
        for x in features.iter_mut() {
            *x = self.inputs.standard_normal();
        }
        self.generated += 1;
        let signal = self
            .coefficients()
            .iter()
            .zip(features.iter())
            .fold(0.0, |sum, (c, x)| sum + c * x);
        signal + NOISE * self.noise.standard_normal()
    }
}

#[cfg(test)]
#[expect(clippy::float_cmp, reason = "tests assert exact, deterministic values")]
mod tests {
    use super::*;

    fn readings(house: &mut House, samples: u64) -> Vec<(u64, usize)> {
        let mut x = [0.0; NUM_SENSORS];
        let mut switched = Vec::new();
        let mut log = Vec::new();
        for _ in 0..samples {
            house.next_sample(&mut x, &mut switched);
            log.extend(switched.iter().map(|&s| (house.generated(), s)));
        }
        log
    }

    #[test]
    fn a_new_house_starts_strong_with_three_inert_sensors() {
        let house = House::new(3);
        assert_eq!(house.seed(), 3);
        assert!((0..RELEVANT).all(|sensor| house.is_strong(sensor)));
        assert_eq!(house.generated(), 0);
        let c = house.coefficients();
        assert_eq!(c[..RELEVANT], [1.2, 0.9, -1.1]);
        assert!(c[RELEVANT..].iter().all(|&v| v == 0.0), "{c:?}");
    }

    #[test]
    fn only_relevant_sensors_switch_and_never_before_the_first_reading() {
        let mut house = House::new(3);
        let log = readings(&mut house, 20_000);
        assert!(log.iter().all(|&(t, s)| t > 1 && s < RELEVANT), "{log:?}");
        let rate = f64::from(u32::try_from(log.len()).expect("small"))
            / (19_999.0 * f64::from(u32::try_from(RELEVANT).expect("small")));
        assert!(
            (rate - 1.0 / 60.0).abs() < 0.003,
            "switch rate {rate}, documented as 1/60"
        );
    }

    #[test]
    fn the_levels_are_the_documented_influences() {
        assert_eq!(LEVELS, [[1.2, 0.2], [0.9, 0.1], [-1.1, -0.1]]);
    }

    #[test]
    fn seed_7_reproduces_its_recorded_readings_and_switches() {
        // Recorded from this implementation. A change here changes every published house
        // result, so it must be deliberate. Targets go through ln and cos in the
        // Box-Muller step, whose last bit differs between platform math libraries, so they
        // are compared to 1e-12. Switch times come from integer arithmetic and are exact.
        let mut house = House::new(7);
        let mut x = [0.0; NUM_SENSORS];
        let mut switched = Vec::new();
        let mut targets = Vec::new();
        let mut log = Vec::new();
        for _ in 0..200 {
            targets.push(house.next_sample(&mut x, &mut switched));
            log.extend(switched.iter().map(|&s| (house.generated(), s)));
        }
        let recorded = [
            13_830_281_868_836_244_833_u64,
            13_820_455_362_515_906_439,
            4_610_336_207_472_215_403,
        ]
        .map(f64::from_bits);
        for (got, want) in targets.iter().zip(recorded) {
            assert!(
                (got - want).abs() <= 1e-12 * want.abs().max(1.0),
                "{got} vs {want}"
            );
        }
        assert_eq!(log[..3], [(42, 2), (49, 2), (83, 1)]);
    }

    #[test]
    fn a_switch_toggles_between_the_two_levels() {
        let mut house = House::new(5);
        let mut x = [0.0; NUM_SENSORS];
        let mut switched = Vec::new();
        // At 1/60 per sensor a switch is due within a few dozen readings. The bound turns a
        // house that never switches into a failure instead of a hang.
        for _ in 0..1_000 {
            let before = house.coefficients();
            house.next_sample(&mut x, &mut switched);
            if let Some(&sensor) = switched.first() {
                let after = house.coefficients();
                assert_ne!(before[sensor], after[sensor]);
                assert!(LEVELS[sensor].contains(&after[sensor]));
                assert_eq!(house.is_strong(sensor), after[sensor] == LEVELS[sensor][0]);
                return;
            }
        }
        panic!("no relevant sensor switched in 1,000 readings");
    }

    #[test]
    fn the_target_is_the_dot_product_plus_bounded_noise() {
        let mut house = House::new(9);
        let mut x = [0.0; NUM_SENSORS];
        let mut switched = Vec::new();
        let mut residuals = Vec::new();
        for _ in 0..5_000 {
            let target = house.next_sample(&mut x, &mut switched);
            let signal: f64 = house
                .coefficients()
                .iter()
                .zip(&x)
                .map(|(c, v)| c * v)
                .sum();
            residuals.push(target - signal);
        }
        let n = f64::from(u32::try_from(residuals.len()).expect("small"));
        let mean = residuals.iter().sum::<f64>() / n;
        let sd = (residuals.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / n).sqrt();
        assert!(mean.abs() < 0.03, "noise mean {mean}");
        assert!((sd - NOISE).abs() < 0.03, "noise sd {sd} vs {NOISE}");
    }

    #[test]
    fn a_still_house_never_switches_and_shares_the_readings() {
        let (mut moving, mut still) = (House::new(2), House::still(2));
        let (mut a, mut b) = ([0.0; NUM_SENSORS], [0.0; NUM_SENSORS]);
        let mut switched = Vec::new();
        for _ in 0..2_000 {
            moving.next_sample(&mut a, &mut switched);
            still.next_sample(&mut b, &mut switched);
            assert!(switched.is_empty(), "the still house switched");
            assert_eq!(a, b, "readings depend only on the seed");
        }
        assert_eq!(still.coefficients()[..RELEVANT], [1.2, 0.9, -1.1]);
    }

    #[test]
    fn the_same_seed_gives_the_same_stream() {
        let (mut a, mut b) = (House::new(11), House::new(11));
        let (mut xa, mut xb) = ([0.0; NUM_SENSORS], [0.0; NUM_SENSORS]);
        let mut switched = Vec::new();
        for _ in 0..500 {
            let ta = a.next_sample(&mut xa, &mut switched);
            let tb = b.next_sample(&mut xb, &mut switched);
            assert_eq!(ta.to_bits(), tb.to_bits());
            assert_eq!(xa, xb);
        }
    }
}
