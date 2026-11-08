//! The tasks a measured run can use: Sutton's tracking task and its extensions.

use crate::world::{self, Event, Preset, World};

/// A synthetic task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Task {
    /// A 20-input tracking task from [`crate::world`].
    Preset(Preset),
}

impl Task {
    /// Returns a stable machine-readable identifier.
    pub const fn id(self) -> &'static str {
        match self {
            Self::Preset(preset) => preset.id(),
        }
    }

    /// Returns the number of inputs per sample.
    pub const fn num_features(self) -> usize {
        match self {
            Self::Preset(_) => world::NUM_FEATURES,
        }
    }
}

impl From<Preset> for Task {
    fn from(preset: Preset) -> Self {
        Self::Preset(preset)
    }
}

/// A seeded stream of samples from a [`Task`].
#[derive(Debug, Clone)]
pub enum Stream {
    /// A tracking task and its event buffer.
    World(World, Vec<Event>),
}

impl Stream {
    /// Creates the stream for `task`. The same task and seed always produce the same
    /// samples.
    pub fn new(task: Task, seed: u64) -> Self {
        match task {
            Task::Preset(preset) => Self::World(World::new(preset, seed), Vec::new()),
        }
    }

    /// Writes the next sample's inputs into `features` and returns its target.
    ///
    /// # Panics
    ///
    /// Panics if `features` does not hold the task's number of inputs.
    pub fn next_sample(&mut self, features: &mut [f64]) -> f64 {
        match self {
            Self::World(world, events) => world.next_sample(features, events),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_and_sizes_follow_the_task() {
        assert_eq!(Task::from(Preset::Drifting).id(), "drifting");
        assert_eq!(Task::Preset(Preset::Sutton1992).num_features(), 20);
    }

    #[test]
    fn a_stream_matches_its_underlying_task() {
        let mut stream = Stream::new(Task::Preset(Preset::Sutton1992), 4);
        let mut world = World::new(Preset::Sutton1992, 4);
        let (mut a, mut b) = ([0.0; 20], [0.0; 20]);
        let mut events = Vec::new();
        for _ in 0..100 {
            let ta = stream.next_sample(&mut a);
            let tb = world.next_sample(&mut b, &mut events);
            assert_eq!(ta.to_bits(), tb.to_bits());
        }
    }
}
