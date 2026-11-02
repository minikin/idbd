//! Experiment support for the `idbd` crate: a versioned random generator, Sutton's
//! (1992) tracking task and extensions, a fixed-step LMS baseline, paired sessions for
//! the IDBD Explorer demo, and measured runs for the reproduction scripts.
//!
//! This crate is not published. All learning goes through the `idbd` core.

pub mod experiment;
pub mod lms;
pub mod rng;
pub mod session;
pub mod trace;
pub mod walkthrough;
pub mod world;
