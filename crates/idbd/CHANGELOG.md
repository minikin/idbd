# Changelog

All notable changes to the `idbd` crate are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the crate follows
[Semantic Versioning](https://semver.org/).

## [Unreleased]

## [0.1.0](https://github.com/minikin/idbd/releases/tag/v0.1.0) - 2026-11-08

### Added

- *(core)* reuse buffers in clone_from for Idbd and Autostep
- *(core)* add the Autostep learner
- *(core)* add an optional serde feature

### Fixed

- address the crate, lab and docs reviews, rework the README and gate CRAP at 10 ([#19](https://github.com/minikin/idbd/pull/19))
- *(core)* require exact float round trips in the serde tests

### Added

- `Idbd`, a dense linear learner implementing equations 3 to 6 of Sutton (1992), with
  `predict`, `update`, inspection of weights, log step sizes, traces and step sizes, and full
  `snapshot` and `from_state` checkpoints.
- `Config` and `Bounds`, validated at construction. `Bounds::sutton_safety()` provides the
  optional beta floor of -10 and per-step limit of 2.
- Typed errors: `ConfigError`, `Error` with the failed `Stage` and coordinate, and `StateError`.
  `ConfigError::TooManyFeatures` rejects a feature count whose vectors could not be allocated.
- `autostep::Autostep`, the Autostep learner of Mahmood, Sutton, Degris and Pilarski (2012),
  with its own `Config`, `State`, `StateError` and `Update`, and the stages `Normalizer` and
  `EffectiveStepSize`.
- `Clone::clone_from` on `Idbd` and `Autostep` reuses the destination's buffers, so a scratch
  copy can be refreshed every sample without allocating.
- Optional `serde` feature: `Serialize` and `Deserialize` for `Config`, `Bounds`, `State` and
  `Idbd`, and for the Autostep types. Deserialization validates configurations, states and learners
  like their constructors and `from_state`.

### Numerical and evaluation limitations

- Equation fixtures agree with an independent 50-digit decimal reference within
  `1e-15 + 1e-13 * max(|a|, |b|)`. Bitwise agreement is promised only for identical replay on
  the same build and target.
- The paper's experiments are reproduced approximately with recorded seeds and a documented
  generator. The paper's own random draws are unpublished, so exact numbers are not reproduced.
