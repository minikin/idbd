//! Incremental Delta-Bar-Delta (IDBD): a linear online learner with one adaptive step
//! size per input.
//!
//! IDBD (Sutton, 1992) is the LMS rule with a separate learning rate `alpha_i` for
//! every input. It adapts `beta_i = ln(alpha_i)` by gradient descent, so inputs whose
//! recent updates keep pointing the same way get larger steps, and inputs whose updates
//! cancel out get smaller ones. This helps on nonstationary streams in which only some
//! inputs carry signal.
//!
//! # Example
//!
//! ```
//! use idbd::{Config, Idbd};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // Two signal inputs, one noise input and a constant 1.0 for the intercept.
//! let config = Config::new(0.05, 0.01)?;
//! let mut learner = Idbd::new(4, config)?;
//!
//! let examples = [
//!     ([0.5, -1.0, 0.3, 1.0], 2.5),
//!     ([1.5, 0.2, -0.7, 1.0], 4.1),
//! ];
//! for (features, target) in &examples {
//!     let report = learner.update(features, *target)?;
//!     // `report.error` was measured before learning from this example.
//!     assert!(report.error.is_finite());
//! }
//!
//! let alphas: Vec<f64> = learner.step_sizes().collect();
//! assert_eq!(alphas.len(), 4);
//! # Ok(())
//! # }
//! ```
//!
//! # The update
//!
//! For features `x`, weights `w`, traces `h`, log step sizes `beta`, meta step size
//! `theta` and a scalar target:
//!
//! ```text
//! prediction = sum_i(w_i * x_i)
//! error      = target - prediction
//! for each i, with the same pre-update error:
//!     beta_i  = beta_i + theta * error * x_i * h_i
//!     alpha_i = exp(beta_i)
//!     w_i     = w_i + alpha_i * error * x_i
//!     h_i     = h_i * max(0, 1 - alpha_i * x_i^2) + alpha_i * error * x_i
//! ```
//!
//! The beta update uses the old trace. The weight and trace updates use the new step
//! size. This follows equations 3 to 6 of the paper. Figure 2's pseudocode prints the
//! trace decay as a sum, `h_i + [1 - alpha_i x_i^2]^+`; equation 6 and the derivation
//! show it is a product, which this crate implements (Sutton 1992, pp. 172-173).
//!
//! # Guarantees
//!
//! - Every operation validates its inputs and every intermediate result. Errors are
//!   typed: [`ConfigError`] for construction, [`Error`] for prediction and update,
//!   [`StateError`] for restoration.
//! - A failed update leaves the committed state bit-for-bit unchanged.
//! - [`Idbd::predict`], [`Idbd::update`] and the inspection methods do not allocate.
//! - [`Idbd::snapshot`] and [`Idbd::from_state`] save and restore the complete state:
//!   configuration, weights, betas and traces.
//! - The crate has no dependencies and no `unsafe` code. It requires `std`.
//!
//! # Autostep
//!
//! The [`autostep`] module implements Autostep (Mahmood et al., 2012), an IDBD variant
//! whose meta step size needs no tuning to the scale of the target, and which never
//! overshoots the current target. It has the same validation, atomicity and
//! allocation guarantees.
//!
//! # Features
//!
//! - `serde`: implements `Serialize` and `Deserialize` for [`Config`], [`Bounds`],
//!   [`State`] and [`Idbd`], and for the [`autostep`] types. Deserializing a `Config` or `Bounds` runs the same checks as
//!   their constructors, and deserializing an `Idbd` runs [`Idbd::from_state`]. A
//!   deserialized `State` is unvalidated, as with [`State::new`]. Off by default, so the
//!   default build has no dependencies. A checkpoint resumes bit for bit only if the
//!   format restores every `f64` exactly. With `serde_json`, enable its
//!   `float_roundtrip` feature: the default parser can change the last digit.
//!
//! # Limitations
//!
//! - **Input and target scaling.** The trace decay `1 - alpha_i * x_i^2` assumes inputs
//!   of moderate magnitude, and the meta step `theta * error * x_i * h_i` grows with the
//!   square of the error scale. Standardize inputs and express the target in units where
//!   typical errors are near 1. Unscaled data can overflow a step size, which
//!   [`Idbd::update`] reports as [`Stage::StepSize`].
//! - **Correlated inputs.** IDBD adapts each step size from that coordinate's own
//!   history. Its derivation assumes that changing one step size mainly affects its own
//!   weight, which is less accurate when inputs are strongly correlated.
//! - **Nonstationary streams.** IDBD tracks drifting targets, but it does not detect
//!   change points. After the relevant inputs change, recovery speed depends on theta and
//!   on how small the step sizes of newly relevant inputs have become.
//! - **Tuning.** Theta is a free parameter. Values that are too large can diverge.
//!   [`Bounds::sutton_safety`] limits beta but does not guarantee convergence.
//! - **Scope.** One linear output, dense `f64` inputs. No sparse inputs, no nonlinear
//!   models and no TD learning.
//!
//! # Reference
//!
//! Richard S. Sutton. *Adapting Bias by Gradient Descent: An Incremental Version of
//! Delta-Bar-Delta.* Proceedings of the Tenth National Conference on Artificial
//! Intelligence (AAAI-92), pp. 171-176, 1992.
//! <https://cdn.aaai.org/AAAI/1992/AAAI92-027.pdf>

pub mod autostep;
mod config;
mod learner;
#[cfg(feature = "serde")]
mod serialization;
mod state;

pub use config::{BoundKind, Bounds, Config, ConfigError};
pub use learner::{Error, Idbd, Stage, StepSizes, Update};
pub use state::{State, StateError};

#[cfg(doctest)]
#[doc = include_str!("../README.md")]
struct ReadmeDoctests;
