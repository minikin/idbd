//! Seeded synthetic tracking tasks with explicit, recorded world events.

use crate::rng::Rng;

/// Number of inputs in every task, as in Sutton (1992), Experiment 1.
pub const NUM_FEATURES: usize = 20;

/// Number of relevant inputs at any time, as in Sutton (1992), Experiment 1.
pub const GROUP_SIZE: usize = 5;

/// Number of disjoint relevance groups: features `5g..5g+5` form group `g`.
pub const NUM_GROUPS: usize = NUM_FEATURES / GROUP_SIZE;

/// Sutton (1992): "every 20 examples one of the five `s_i` was selected at random and
/// switched in sign".
pub const SIGN_FLIP_PERIOD: u64 = 20;

/// Samples between scheduled relevance shifts in the changing-relevance task.
pub const RELEVANCE_SHIFT_PERIOD: u64 = 5_000;

/// Autocorrelation of the drifting task's coefficients per sample. The stationary
/// marginal distribution of each relevant coefficient stays standard normal.
pub const DRIFT_RHO: f64 = 0.999;

/// Seed offset for the world-event stream, so user-triggered events never change the
/// input stream.
const WORLD_STREAM: u64 = 0x5745_4F52_4C44_0001;

/// A synthetic target-generating process.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Preset {
    /// Sutton (1992), Experiment 1: the first five coefficients are each +1 or -1 and
    /// one of them, chosen at random, flips sign every 20 samples. The other fifteen
    /// are zero.
    Sutton1992,
    /// Five fixed signed coefficients on the first group. Nothing changes.
    Stationary,
    /// Extension: Sutton's sign flips, plus every 5,000 samples the relevance moves to
    /// the next group of five inputs. Not part of the paper.
    ChangingRelevance,
    /// Extension: the five relevant coefficients follow independent AR(1) processes
    /// with autocorrelation 0.999 and unit stationary variance. Not part of the paper.
    Drifting,
}

impl Preset {
    /// Every preset, in display order.
    pub const ALL: [Self; 4] = [
        Self::Sutton1992,
        Self::Stationary,
        Self::ChangingRelevance,
        Self::Drifting,
    ];

    /// Returns a stable machine-readable identifier.
    pub const fn id(self) -> &'static str {
        match self {
            Self::Sutton1992 => "sutton1992",
            Self::Stationary => "stationary",
            Self::ChangingRelevance => "changing-relevance",
            Self::Drifting => "drifting",
        }
    }

    /// Parses an identifier produced by [`Preset::id`].
    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|preset| preset.id() == id)
    }

    /// Returns `true` for tasks from Sutton (1992) and `false` for new extensions.
    pub const fn is_from_paper(self) -> bool {
        matches!(self, Self::Sutton1992)
    }

    const fn flips_signs(self) -> bool {
        matches!(self, Self::Sutton1992 | Self::ChangingRelevance)
    }
}

/// A change to the target-generating relationship, applied before a sample.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Event {
    /// A relevant coefficient changed sign.
    SignFlip {
        /// The affected feature.
        feature: usize,
    },
    /// The relevant group moved. Old coefficients become zero and the new group gets
    /// fresh random signs.
    RelevanceShift {
        /// The previously relevant group.
        from: usize,
        /// The newly relevant group.
        to: usize,
        /// Whether the preset scheduled the shift or a user requested it.
        origin: Origin,
    },
}

/// Who caused an event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Origin {
    /// The preset's schedule.
    Scheduled,
    /// A user action, such as "Change the world".
    Requested,
}

/// A seeded task that produces one example at a time.
///
/// Inputs are independent standard normals. The target is the dot product of the
/// inputs with the current true coefficients, without noise.
#[derive(Debug, Clone)]
pub struct World {
    preset: Preset,
    seed: u64,
    coefficients: Vec<f64>,
    group: usize,
    inputs: Rng,
    changes: Rng,
    generated: u64,
    shift_requested: bool,
}

impl World {
    /// Creates a world. The same preset and seed always produce the same stream.
    pub fn new(preset: Preset, seed: u64) -> Self {
        let mut changes = Rng::new(seed ^ WORLD_STREAM);
        let mut coefficients = vec![0.0; NUM_FEATURES];
        for c in &mut coefficients[..GROUP_SIZE] {
            *c = match preset {
                Preset::Drifting => changes.standard_normal(),
                _ => changes.sign(),
            };
        }
        Self {
            preset,
            seed,
            coefficients,
            group: 0,
            inputs: Rng::new(seed),
            changes,
            generated: 0,
            shift_requested: false,
        }
    }

    /// Returns the preset.
    pub fn preset(&self) -> Preset {
        self.preset
    }

    /// Returns the seed.
    pub fn seed(&self) -> u64 {
        self.seed
    }

    /// Returns the coefficients that generated the most recent sample.
    pub fn coefficients(&self) -> &[f64] {
        &self.coefficients
    }

    /// Returns the index of the currently relevant group.
    pub fn relevant_group(&self) -> usize {
        self.group
    }

    /// Returns the number of samples generated so far.
    pub fn generated(&self) -> u64 {
        self.generated
    }

    /// Schedules a relevance shift to the next group before the next sample.
    ///
    /// Requesting twice before a sample still produces one shift.
    pub fn request_relevance_shift(&mut self) {
        self.shift_requested = true;
    }

    /// Returns `true` if a requested shift waits for the next sample.
    pub fn shift_pending(&self) -> bool {
        self.shift_requested
    }

    /// Applies the events due before the next sample, then writes its inputs into
    /// `features` and returns its target.
    ///
    /// `events` is cleared and receives the events applied before this sample.
    ///
    /// # Panics
    ///
    /// Panics if `features` does not hold [`NUM_FEATURES`] values.
    pub fn next_sample(&mut self, features: &mut [f64], events: &mut Vec<Event>) -> f64 {
        assert_eq!(features.len(), NUM_FEATURES, "feature buffer length");
        events.clear();
        let t = self.generated + 1;
        let boundary = |period: u64| t > 1 && (t - 1) % period == 0;
        if self.shift_requested {
            self.shift_requested = false;
            events.push(self.shift(Origin::Requested));
        }
        if self.preset == Preset::ChangingRelevance && boundary(RELEVANCE_SHIFT_PERIOD) {
            events.push(self.shift(Origin::Scheduled));
        }
        if self.preset.flips_signs() && boundary(SIGN_FLIP_PERIOD) {
            let feature = self.group * GROUP_SIZE + self.changes.index(GROUP_SIZE);
            self.coefficients[feature] = -self.coefficients[feature];
            events.push(Event::SignFlip { feature });
        }
        if self.preset == Preset::Drifting && t > 1 {
            let innovation = (1.0 - DRIFT_RHO * DRIFT_RHO).sqrt();
            let start = self.group * GROUP_SIZE;
            for c in &mut self.coefficients[start..start + GROUP_SIZE] {
                *c = DRIFT_RHO * *c + innovation * self.changes.standard_normal();
            }
        }
        for x in features.iter_mut() {
            *x = self.inputs.standard_normal();
        }
        self.generated = t;
        self.coefficients
            .iter()
            .zip(features.iter())
            .fold(0.0, |sum, (c, x)| sum + c * x)
    }

    fn shift(&mut self, origin: Origin) -> Event {
        let from = self.group;
        let to = (from + 1) % NUM_GROUPS;
        let (old, new) = (from * GROUP_SIZE, to * GROUP_SIZE);
        let carried: [f64; GROUP_SIZE] = std::array::from_fn(|i| self.coefficients[old + i]);
        self.coefficients[old..old + GROUP_SIZE].fill(0.0);
        for (i, c) in self.coefficients[new..new + GROUP_SIZE]
            .iter_mut()
            .enumerate()
        {
            *c = match self.preset {
                Preset::Drifting => carried[i],
                _ => self.changes.sign(),
            };
        }
        self.group = to;
        Event::RelevanceShift { from, to, origin }
    }
}

#[cfg(test)]
#[expect(clippy::float_cmp, reason = "tests assert exact, deterministic values")]
mod tests {
    use super::*;

    fn run(world: &mut World, samples: u64) -> Vec<(u64, Event)> {
        let mut features = [0.0; NUM_FEATURES];
        let mut events = Vec::new();
        let mut log = Vec::new();
        for _ in 0..samples {
            world.next_sample(&mut features, &mut events);
            log.extend(events.iter().map(|e| (world.generated(), *e)));
        }
        log
    }

    #[test]
    fn sutton_task_flips_one_relevant_sign_every_20_samples() {
        let mut world = World::new(Preset::Sutton1992, 1);
        let initial = world.coefficients().to_vec();
        assert!(initial[..5].iter().all(|c| c.abs() == 1.0), "{initial:?}");
        assert!(initial[5..].iter().all(|&c| c == 0.0), "{initial:?}");
        let log = run(&mut world, 100);
        let times: Vec<u64> = log.iter().map(|(t, _)| *t).collect();
        assert_eq!(
            times,
            vec![21, 41, 61, 81],
            "flips apply before these samples"
        );
        for (_, event) in &log {
            assert!(
                matches!(event, Event::SignFlip { feature } if *feature < 5),
                "{event:?}"
            );
        }
    }

    #[test]
    fn target_is_the_noise_free_dot_product() {
        let mut world = World::new(Preset::Stationary, 9);
        let mut features = [0.0; NUM_FEATURES];
        let mut events = Vec::new();
        for _ in 0..50 {
            let target = world.next_sample(&mut features, &mut events);
            let expected: f64 = world
                .coefficients()
                .iter()
                .zip(&features)
                .map(|(c, x)| c * x)
                .sum();
            assert!((target - expected).abs() < 1e-12, "{target} vs {expected}");
            assert!(events.is_empty(), "stationary task has no events");
        }
    }

    #[test]
    fn requested_shift_preserves_the_input_stream() {
        let mut plain = World::new(Preset::Sutton1992, 4);
        let mut shifted = World::new(Preset::Sutton1992, 4);
        let (mut a, mut b) = ([0.0; NUM_FEATURES], [0.0; NUM_FEATURES]);
        let mut events = Vec::new();
        for t in 1..=60 {
            if t == 30 {
                shifted.request_relevance_shift();
                shifted.request_relevance_shift();
                assert!(shifted.shift_pending(), "pending");
            }
            plain.next_sample(&mut a, &mut events);
            shifted.next_sample(&mut b, &mut events);
            if t == 30 {
                assert_eq!(
                    events.first(),
                    Some(&Event::RelevanceShift {
                        from: 0,
                        to: 1,
                        origin: Origin::Requested
                    }),
                    "one requested shift before sample 30"
                );
                assert_eq!(events.len(), 1, "{events:?}");
            }
            assert_eq!(a, b, "inputs at sample {t}");
        }
        assert_eq!(shifted.relevant_group(), 1);
        let c = shifted.coefficients();
        assert!(c[..5].iter().all(|&v| v == 0.0), "old group cleared: {c:?}");
        assert!(
            c[5..10].iter().all(|v| v.abs() == 1.0),
            "new group signed: {c:?}"
        );
    }

    #[test]
    fn changing_relevance_shifts_on_schedule_and_cycles() {
        let mut world = World::new(Preset::ChangingRelevance, 2);
        let log = run(&mut world, 4 * RELEVANCE_SHIFT_PERIOD + 1);
        let shifts: Vec<(u64, usize)> = log
            .iter()
            .filter_map(|(t, e)| match e {
                Event::RelevanceShift { to, .. } => Some((*t, *to)),
                Event::SignFlip { .. } => None,
            })
            .collect();
        assert_eq!(
            shifts,
            vec![(5_001, 1), (10_001, 2), (15_001, 3), (20_001, 0)]
        );
    }

    #[test]
    fn drifting_coefficients_move_with_unit_scale() {
        let mut world = World::new(Preset::Drifting, 3);
        let start = world.coefficients()[0];
        run(&mut world, 5_000);
        let end = world.coefficients()[0];
        assert_ne!(start, end, "coefficients drift");
        assert!(
            world.coefficients()[..5].iter().all(|c| c.abs() < 6.0),
            "bounded scale"
        );
        assert!(
            world.coefficients()[5..].iter().all(|&c| c == 0.0),
            "irrelevant stay zero"
        );
    }

    #[test]
    fn presets_round_trip_their_ids() {
        for preset in Preset::ALL {
            assert_eq!(Preset::from_id(preset.id()), Some(preset));
        }
        assert_eq!(Preset::from_id("unknown"), None);
        assert!(Preset::Sutton1992.is_from_paper());
        assert!(!Preset::ChangingRelevance.is_from_paper());
    }
}
