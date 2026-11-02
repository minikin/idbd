//! Executable core scenarios from `features/idbd.feature`.
//!
//! Test names start with the scenario ID they implement, such as `c06_...`.

#![expect(
    clippy::float_cmp,
    reason = "scenarios specify exact values for zero errors, unchanged betas and bounds"
)]

mod bounds;
mod construction;
mod failures;
mod fixtures;
mod messages;
mod prediction;
mod properties;
mod recurrence;
mod snapshot;
mod support;
